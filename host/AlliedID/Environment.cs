using System.Text;
using System.Text.Json;

namespace AlliedID;

public sealed record EnvDoc(string Id, string Title, string Source, string Text);

/// <summary>
/// 데이터 생성용 환경 관찰. 이 컴퓨터, 네트워크, 프로젝트 구조, 프로젝트 문서를
/// 문장으로 옮겨 본체의 데이터 생성기(Forge)에 넘긴다. 외부에서 가져오는 것은 없다.
/// </summary>
public static class EnvironmentCollector
{
    public static List<EnvDoc> Collect(NetReport net)
    {
        var docs = new List<EnvDoc>();
        var sys = SystemProbe.Info();
        docs.Add(new("env:system", "이 컴퓨터", "환경 관찰",
            $"이 컴퓨터의 운영체제는 {sys["os"]}이고 아키텍처는 {sys["arch"]}예요. CPU 코어는 {sys["cpus"]}개, 메모리는 {sys["memory_gb"]}GB예요. " +
            $"컴퓨터 이름은 {sys["machine"]}이고 사용자는 {sys["user"]}예요. 프로젝트가 있는 디스크의 여유 공간은 {sys["disk_free_gb"]}GB예요."));

        var net_text = new StringBuilder(net.Online ? "시작할 때 이 컴퓨터는 인터넷에 닿아 있었어요. " : "시작할 때 이 컴퓨터는 인터넷에 닿지 않았어요. ");
        foreach (var i in net.Interfaces.Where(i => i.Up && i.Kind != "virtual"))
            net_text.Append($"{i.Name} 인터페이스는 {KindKo(i.Kind)} 연결이에요. ");
        if (net.Wifi is { } w) net_text.Append($"연결된 Wi-Fi 이름은 {w.Ssid}이고 보안 방식은 {w.Security}예요. ");
        foreach (var warn in net.Warnings) net_text.Append(warn).Append(' ');
        docs.Add(new("env:network", "시작할 때의 네트워크", "환경 관찰", net_text.ToString()));

        var files = ProjectFiles.All().ToList();
        var top = files.GroupBy(f => f.Contains('/') ? f[..f.IndexOf('/')] + "/" : f).Select(g => $"{g.Key} ({g.Count()}개)");
        docs.Add(new("env:project", "프로젝트 구조", "환경 관찰",
            $"이 프로젝트는 {ProjectPaths.Root} 에 있고 파일이 {files.Count}개 있어요. 최상위 구성은 {string.Join(", ", top)}이에요. " +
            "core 폴더는 Rust로 만든 본체 엔진, host 폴더는 C#으로 만든 호스트, data 폴더는 씨앗 데이터와 생성된 데이터예요."));

        var readme = Path.Combine(ProjectPaths.Root, "README.md");
        if (File.Exists(readme))
            docs.Add(new("project:README.md", "README", "README.md", File.ReadAllText(readme)));
        return docs;
    }

    static string KindKo(string kind) => kind switch
    {
        "wifi" => "Wi-Fi", "ethernet" => "유선", "cellular" => "셀룰러", "tethering" => "테더링", "vpn" => "VPN", _ => kind,
    };
}

/// <summary>본체가 tool_use 로 요청하는 관찰·행동을 실제로 수행한다.</summary>
public sealed class HostTools(IdentityStore identity)
{
    static readonly string[] Days = ["일", "월", "화", "수", "목", "금", "토"];

    public (object Content, bool IsError) Run(string name, JsonElement input)
    {
        try
        {
            return name switch
            {
                "time.now" => (Now(), false),
                "net.status" => (NetworkProbe.Scan(), false),
                "system.info" => (SystemProbe.Info(), false),
                "project.files" => (ProjectFiles.Search(Str(input, "query")), false),
                "memory.save" => (new { memory = identity.Remember(Str(input, "text")) }, false),
                "memory.forget" => (new { removed = identity.Forget(Str(input, "query")) }, false),
                "identity.set_owner" => SetOwner(Str(input, "name")),
                _ => ($"알 수 없는 도구: {name}", true),
            };
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or JsonException)
        {
            return (e.Message, true);
        }
    }

    (object, bool) SetOwner(string name)
    {
        identity.SetOwner(name);
        return (new { name }, false);
    }

    static string Str(JsonElement input, string key) =>
        input.ValueKind == JsonValueKind.Object && input.TryGetProperty(key, out var v) ? v.GetString() ?? "" : "";

    static object Now()
    {
        var t = DateTime.Now;
        var hour = t.Hour % 12 == 0 ? 12 : t.Hour % 12;
        return new
        {
            time = $"{(t.Hour < 12 ? "오전" : "오후")} {hour}시 {t.Minute}분",
            date = $"{t.Year}년 {t.Month}월 {t.Day}일 {Days[(int)t.DayOfWeek]}요일",
            iso = t.ToString("yyyy-MM-ddTHH:mm:ss"),
        };
    }
}
