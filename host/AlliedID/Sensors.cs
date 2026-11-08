using System.Diagnostics;
using System.Net.NetworkInformation;
using System.Net.Sockets;
using System.Runtime.InteropServices;

namespace AlliedID;

public sealed record Iface(string Name, string Kind, bool Up, List<string> Addresses);
public sealed record WifiInfo(string Ssid, string Security, int? Signal, bool Open);

public sealed class NetReport
{
    public bool Online { get; set; }
    public List<string> Reachable { get; set; } = [];
    public List<Iface> Interfaces { get; set; } = [];
    public WifiInfo? Wifi { get; set; }
    public List<string> Cellular { get; set; } = [];
    public List<string> Warnings { get; set; } = [];
}

/// <summary>
/// 네트워크 환경 관찰. Wi-Fi·네트워크는 인터넷을 쓰려는 것이 아니라, 이 컴퓨터가 지금
/// 어떤 네트워크에 노출되어 있는지 파악하기 위한 것이다. 인터넷 도달 여부는 TCP 접속만 시험한다.
/// </summary>
public static class NetworkProbe
{
    static readonly string[] Targets = ["1.1.1.1:443", "8.8.8.8:443"];

    public static NetReport Scan()
    {
        var r = new NetReport();
        foreach (var nic in NetworkInterface.GetAllNetworkInterfaces())
        {
            if (nic.NetworkInterfaceType == NetworkInterfaceType.Loopback) continue;
            var addrs = nic.GetIPProperties().UnicastAddresses.Select(a => $"{a.Address}/{a.PrefixLength}").ToList();
            r.Interfaces.Add(new Iface(nic.Name, Classify(nic), nic.OperationalStatus == OperationalStatus.Up, addrs));
        }
        r.Interfaces.Sort((a, b) => string.CompareOrdinal(a.Name, b.Name));
        r.Wifi = OperatingSystem.IsWindows() ? WindowsWifi() : OperatingSystem.IsLinux() ? NmcliWifi() : null;
        if (OperatingSystem.IsLinux() && Run("mmcli", "-L") is { } modems)
            r.Cellular = modems.Split('\n').Select(l => l.Trim()).Where(l => l.Contains("/Modem/")).ToList();
        r.Reachable = Probe(TimeSpan.FromMilliseconds(1200));
        r.Online = r.Reachable.Count > 0;
        r.Warnings = Analyze(r);
        return r;
    }

    static string Classify(NetworkInterface nic)
    {
        var name = nic.Name.ToLowerInvariant();
        var desc = nic.Description.ToLowerInvariant();
        if (OperatingSystem.IsLinux())
        {
            var sys = $"/sys/class/net/{nic.Name}";
            var driver = Directory.Exists($"{sys}/device/driver")
                ? Path.GetFileName(new DirectoryInfo($"{sys}/device/driver").ResolveLinkTarget(true)?.FullName ?? "")
                : "";
            if (Directory.Exists($"{sys}/wireless") || Directory.Exists($"{sys}/phy80211")) return "wifi";
            if (name.StartsWith("wwan") || name.StartsWith("ppp") || driver is "qmi_wwan" or "cdc_mbim" or "option") return "cellular";
            if (name.StartsWith("usb") || driver is "rndis_host" or "ipheth") return "tethering";
            if (new[] { "tun", "tap", "wg", "tailscale", "zt" }.Any(name.StartsWith)) return "vpn";
            if (!Directory.Exists($"{sys}/device") || new[] { "docker", "br-", "virbr", "veth", "vmnet", "lxc" }.Any(name.StartsWith)) return "virtual";
            return "ethernet";
        }
        return nic.NetworkInterfaceType switch
        {
            NetworkInterfaceType.Wireless80211 => "wifi",
            NetworkInterfaceType.Wwanpp or NetworkInterfaceType.Wwanpp2 or NetworkInterfaceType.Ppp => "cellular",
            NetworkInterfaceType.Tunnel => "vpn",
            _ when desc.Contains("rndis") || desc.Contains("tether") => "tethering",
            _ when desc.Contains("vpn") || desc.Contains("wireguard") || new[] { "utun", "tun", "wg" }.Any(name.StartsWith) => "vpn",
            _ when desc.Contains("virtual") || desc.Contains("hyper-v") || new[] { "vethernet", "bridge", "vmnet" }.Any(name.StartsWith) => "virtual",
            NetworkInterfaceType.Ethernet or NetworkInterfaceType.GigabitEthernet => "ethernet",
            _ => "other",
        };
    }

    static List<string> Probe(TimeSpan timeout)
    {
        var tasks = Targets.Select(async t =>
        {
            var i = t.LastIndexOf(':');
            try
            {
                using var c = new TcpClient();
                using var cts = new CancellationTokenSource(timeout);
                await c.ConnectAsync(t[..i], int.Parse(t[(i + 1)..]), cts.Token);
                return t;
            }
            catch { return null; }
        }).ToArray();
        Task.WaitAll(tasks, timeout + TimeSpan.FromMilliseconds(300));
        return tasks.Where(t => t.IsCompletedSuccessfully && t.Result is not null).Select(t => t.Result!).Order().ToList();
    }

    static List<string> Analyze(NetReport r)
    {
        var w = new List<string>();
        var kinds = r.Interfaces.Where(i => i.Up && i.Kind != "virtual").Select(i => i.Kind).ToHashSet();
        if (r.Wifi is { } wifi)
        {
            if (wifi.Open) w.Add($"암호가 없는 개방형 Wi-Fi(\"{wifi.Ssid}\")에 연결되어 있어요. 같은 네트워크의 다른 기기가 트래픽을 볼 수 있어요.");
            else if (wifi.Security.Contains("WEP") || wifi.Security.Contains("TKIP")) w.Add($"Wi-Fi 보안 방식({wifi.Security})이 오래되어 취약해요.");
            if (wifi.Signal is < 30) w.Add($"Wi-Fi 신호가 약해요({wifi.Signal}%).");
        }
        if (kinds.Contains("cellular") || kinds.Contains("tethering"))
            w.Add("셀룰러/테더링 경로가 켜져 있어요. 요금제 데이터가 소모될 수 있어요.");
        if (!r.Online && kinds.Any(k => k != "vpn"))
            w.Add("네트워크에 연결은 됐지만 인터넷에 닿지 않아요(캡티브 포털이나 차단된 네트워크일 수 있어요).");
        return w;
    }

    internal static string? Run(string file, string args)
    {
        try
        {
            var psi = new ProcessStartInfo(file, args) { RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false };
            // 출력이 번역되지 않게 메시지 로캘만 C 로 고정 (nmcli 의 "yes"가 "예"가 되는 문제). 문자 집합은 유지.
            psi.Environment.Remove("LC_ALL");
            psi.Environment.Remove("LANGUAGE");
            psi.Environment["LC_MESSAGES"] = "C";
            using var p = Process.Start(psi);
            if (p is null) return null;
            var output = p.StandardOutput.ReadToEnd();
            p.WaitForExit(5000);
            return p.ExitCode == 0 ? output : null;
        }
        catch { return null; }
    }

    static WifiInfo? NmcliWifi()
    {
        var output = Run("nmcli", "-t -f ACTIVE,SSID,SECURITY,SIGNAL dev wifi list --rescan no");
        foreach (var line in output?.Split('\n') ?? [])
        {
            var f = SplitNmcli(line);
            if (f.Count < 4 || f[0] != "yes") continue;
            var security = f[2].Trim();
            var open = security is "" or "--";
            return new WifiInfo(f[1], open ? "없음" : security, int.TryParse(f[3], out var s) ? s : null, open);
        }
        return null;
    }

    static List<string> SplitNmcli(string line)
    {
        var fields = new List<string> { "" };
        for (var i = 0; i < line.Length; i++)
        {
            if (line[i] == '\\' && i + 1 < line.Length) fields[^1] += line[++i];
            else if (line[i] == ':') fields.Add("");
            else fields[^1] += line[i];
        }
        return fields;
    }

    static WifiInfo? WindowsWifi()
    {
        var output = Run("netsh", "wlan show interfaces");
        if (output is null) return null;
        string? ssid = null, auth = null;
        int? signal = null;
        foreach (var raw in output.Split('\n'))
        {
            var kv = raw.Split(':', 2);
            if (kv.Length != 2) continue;
            var (k, v) = (kv[0].Trim(), kv[1].Trim());
            if (k == "SSID") ssid = v;
            else if (k.StartsWith("Authentication") || k.StartsWith("인증")) auth = v;
            else if ((k.StartsWith("Signal") || k.StartsWith("신호")) && int.TryParse(v.TrimEnd('%'), out var s)) signal = s;
        }
        if (ssid is null) return null;
        var open = auth is null || auth.Equals("Open", StringComparison.OrdinalIgnoreCase) || auth == "개방";
        return new WifiInfo(ssid, open ? "없음" : auth!, signal, open);
    }
}

public static class SystemProbe
{
    public static Dictionary<string, object> Info()
    {
        var mem = GC.GetGCMemoryInfo().TotalAvailableMemoryBytes;
        long free = 0;
        try { free = new DriveInfo(Path.GetPathRoot(ProjectPaths.Root)!).AvailableFreeSpace; } catch { }
        return new()
        {
            ["os"] = OsName(),
            ["arch"] = RuntimeInformation.OSArchitecture.ToString(),
            ["cpus"] = Environment.ProcessorCount,
            ["memory_gb"] = Math.Round(mem / 1e9, 1),
            ["disk_free_gb"] = Math.Round(free / 1e9, 1),
            ["uptime_hours"] = Math.Round(Environment.TickCount64 / 3.6e6, 1),
            ["machine"] = Environment.MachineName,
            ["user"] = Environment.UserName,
        };
    }

    static string OsName()
    {
        if (OperatingSystem.IsLinux() && File.Exists("/etc/os-release"))
            foreach (var line in File.ReadLines("/etc/os-release"))
                if (line.StartsWith("PRETTY_NAME="))
                    return line["PRETTY_NAME=".Length..].Trim('"');
        return RuntimeInformation.OSDescription;
    }
}

public static class ProjectFiles
{
    static readonly string[] Skip = [".git", ".build", ".vscode", "target", "bin", "obj", "generated", "state"];

    public static IEnumerable<string> All()
    {
        var stack = new Stack<string>([ProjectPaths.Root]);
        while (stack.Count > 0)
        {
            var dir = stack.Pop();
            foreach (var f in Directory.EnumerateFiles(dir).Order())
                yield return Path.GetRelativePath(ProjectPaths.Root, f).Replace('\\', '/');
            foreach (var d in Directory.EnumerateDirectories(dir).OrderDescending())
                if (!Skip.Contains(Path.GetFileName(d))) stack.Push(d);
        }
    }

    public static Dictionary<string, object> Search(string query, int limit = 15)
    {
        var words = query.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        var files = All().ToList();
        var matches = words.Length == 0
            ? files
            : files.Where(f => words.Any(w => f.Contains(w, StringComparison.OrdinalIgnoreCase))).ToList();
        return new() { ["query"] = query, ["matches"] = matches.Take(limit).ToList(), ["total"] = matches.Count };
    }
}
