using System.Diagnostics;
using System.Net.NetworkInformation;
using System.Net.Sockets;

namespace AlliedID.Companion;

public sealed record Iface(string Name, string Kind, bool Up, List<string> Addresses);
public sealed record WifiInfo(string Ssid, string Security, int? Signal, bool Open);

/// <summary>schemas/net-report.schema.json 과 같은 형식. Rust 코어가 그대로 읽는다.</summary>
public sealed class NetReport
{
    public string Source { get; set; } = "companion";
    public bool Online { get; set; }
    public List<string> Reachable { get; set; } = [];
    public List<Iface> Interfaces { get; set; } = [];
    public WifiInfo? Wifi { get; set; }
    public List<string> Cellular { get; set; } = [];
    public List<string> Warnings { get; set; } = [];
}

/// <summary>
/// .NET 의 NetworkInterface API 로 Windows/macOS/Linux 공통 네트워크 정보를 수집한다.
/// Rust 코어가 OS 를 직접 조사할 수 없는 플랫폼(Windows, macOS)에서 사용된다.
/// </summary>
public static class NetworkProbe
{
    public static readonly string[] DefaultTargets = ["api.anthropic.com:443", "1.1.1.1:443"];

    public static NetReport Scan(bool probe, IEnumerable<string>? targets = null)
    {
        var report = new NetReport();
        foreach (var nic in NetworkInterface.GetAllNetworkInterfaces())
        {
            if (nic.NetworkInterfaceType == NetworkInterfaceType.Loopback) continue;
            var addresses = nic.GetIPProperties().UnicastAddresses
                .Select(a => $"{a.Address}/{a.PrefixLength}")
                .ToList();
            report.Interfaces.Add(new Iface(nic.Name, Classify(nic), nic.OperationalStatus == OperationalStatus.Up, addresses));
        }
        report.Interfaces.Sort((a, b) => string.CompareOrdinal(a.Name, b.Name));
        report.Wifi = OperatingSystem.IsWindows() ? WindowsWifi() : OperatingSystem.IsLinux() ? NmcliWifi() : null;

        if (probe)
        {
            report.Reachable = Probe(targets ?? DefaultTargets, TimeSpan.FromMilliseconds(1500));
            report.Online = report.Reachable.Count > 0;
            report.Warnings = Analyze(report);
        }
        return report;
    }

    static string Classify(NetworkInterface nic)
    {
        var name = nic.Name.ToLowerInvariant();
        var desc = nic.Description.ToLowerInvariant();
        return nic.NetworkInterfaceType switch
        {
            NetworkInterfaceType.Wireless80211 => "wifi",
            NetworkInterfaceType.Wwanpp or NetworkInterfaceType.Wwanpp2 or NetworkInterfaceType.Ppp => "cellular",
            NetworkInterfaceType.Tunnel => "vpn",
            _ when desc.Contains("rndis") || desc.Contains("tether") || name.StartsWith("usb") => "tethering",
            _ when new[] { "tun", "tap", "wg", "utun", "tailscale", "zt" }.Any(name.StartsWith)
                   || desc.Contains("vpn") || desc.Contains("wireguard") => "vpn",
            _ when new[] { "docker", "br-", "virbr", "veth", "vmnet", "vethernet", "bridge" }.Any(name.StartsWith)
                   || desc.Contains("hyper-v") || desc.Contains("virtual") => "virtual",
            NetworkInterfaceType.Ethernet or NetworkInterfaceType.GigabitEthernet
                or NetworkInterfaceType.FastEthernetT or NetworkInterfaceType.FastEthernetFx => "ethernet",
            _ => "other",
        };
    }

    public static List<string> Probe(IEnumerable<string> targets, TimeSpan timeout)
    {
        var tasks = targets.Select(async target =>
        {
            var (host, port) = SplitHostPort(target);
            try
            {
                using var client = new TcpClient();
                using var cts = new CancellationTokenSource(timeout);
                await client.ConnectAsync(host, port, cts.Token);
                return target;
            }
            catch { return null; }
        }).ToArray();
        Task.WaitAll(tasks, timeout + TimeSpan.FromMilliseconds(500));
        return tasks.Where(t => t.IsCompletedSuccessfully && t.Result is not null)
            .Select(t => t.Result!).Order().ToList();
    }

    static (string Host, int Port) SplitHostPort(string target)
    {
        var i = target.LastIndexOf(':');
        return i > 0 && int.TryParse(target[(i + 1)..], out var port) ? (target[..i], port) : (target, 443);
    }

    /// <summary>Rust net::analyze 와 같은 규칙.</summary>
    public static List<string> Analyze(NetReport r)
    {
        var w = new List<string>();
        var kinds = r.Interfaces.Where(i => i.Up && i.Kind != "virtual").Select(i => i.Kind).ToHashSet();
        if (r.Wifi is { } wifi)
        {
            if (wifi.Open)
                w.Add($"암호가 없는 개방형 Wi-Fi(\"{wifi.Ssid}\")에 연결되어 있습니다. 같은 네트워크의 다른 기기가 트래픽을 볼 수 있습니다.");
            else if (wifi.Security.Contains("WEP") || wifi.Security.Contains("TKIP"))
                w.Add($"Wi-Fi 보안 방식({wifi.Security})이 오래되어 취약합니다.");
            if (wifi.Signal is < 30)
                w.Add($"Wi-Fi 신호가 약합니다({wifi.Signal}%).");
        }
        if (kinds.Contains("cellular") || kinds.Contains("tethering"))
            w.Add("셀룰러/테더링 경로가 활성화되어 있습니다. 사용자의 요금제 데이터가 소모될 수 있습니다.");
        if (!r.Online && kinds.Any(k => k != "vpn"))
            w.Add("네트워크 연결은 있지만 인터넷에 닿지 않습니다(캡티브 포털이나 차단된 네트워크일 수 있음).");
        return w;
    }

    static string? Run(string file, string args)
    {
        try
        {
            using var p = Process.Start(new ProcessStartInfo(file, args)
            {
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                UseShellExecute = false,
            });
            if (p is null) return null;
            var output = p.StandardOutput.ReadToEnd();
            p.WaitForExit(5000);
            return p.ExitCode == 0 ? output : null;
        }
        catch { return null; }
    }

    /// <summary>`netsh wlan show interfaces` 의 SSID / 인증 / 신호 줄을 읽는다.</summary>
    static WifiInfo? WindowsWifi()
    {
        var output = Run("netsh", "wlan show interfaces");
        if (output is null) return null;
        string? ssid = null, auth = null;
        int? signal = null;
        foreach (var raw in output.Split('\n'))
        {
            var parts = raw.Split(':', 2);
            if (parts.Length != 2) continue;
            var key = parts[0].Trim();
            var value = parts[1].Trim();
            if (key == "SSID") ssid = value;
            else if (key.StartsWith("Authentication") || key.StartsWith("인증")) auth = value;
            else if ((key.StartsWith("Signal") || key.StartsWith("신호")) && int.TryParse(value.TrimEnd('%'), out var s)) signal = s;
        }
        if (ssid is null) return null;
        var open = auth is null || auth.Equals("Open", StringComparison.OrdinalIgnoreCase) || auth == "개방";
        return new WifiInfo(ssid, open ? "없음" : auth!, signal, open);
    }

    static WifiInfo? NmcliWifi()
    {
        var output = Run("nmcli", "-t -f ACTIVE,SSID,SECURITY,SIGNAL dev wifi list --rescan no");
        if (output is null) return null;
        foreach (var line in output.Split('\n'))
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
}
