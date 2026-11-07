using System.Text.Json;
using AlliedID.Companion;
using AlliedID.Identity;

// alliedid-companion — AlliedID 의 C# 동반 도구.
//   identity : 인격체 프로필·기억 관리 (Rust 코어와 같은 identity.json 공유)
//   net      : 크로스 플랫폼 네트워크 환경 수집 (Windows/macOS 에서 Rust 코어가 위임)

Console.OutputEncoding = System.Text.Encoding.UTF8;

const string Help = """
alliedid-companion — AlliedID 동반 도구

사용법:
  alliedid-companion identity [show] [--json]
  alliedid-companion identity remember <내용> [--tag <태그>]...
  alliedid-companion identity forget <id>
  alliedid-companion identity owner <이름>
  alliedid-companion identity path
  alliedid-companion net [--json] [--no-probe]
  alliedid-companion version
""";

try
{
    return args switch
    {
        [] or ["help" or "-h" or "--help"] => Print(Help),
        ["version" or "--version"] => Print("alliedid-companion 0.1.0"),
        ["identity", .. var rest] => Identity(rest),
        ["net", .. var rest] => Net(rest),
        _ => Fail($"알 수 없는 명령: {string.Join(' ', args)}\n\n{Help}"),
    };
}
catch (Exception e) when (e is IOException or JsonException or InvalidDataException or UnauthorizedAccessException)
{
    return Fail(e.Message);
}

static int Print(string text)
{
    Console.WriteLine(text);
    return 0;
}

static int Fail(string message)
{
    Console.Error.WriteLine($"오류: {message}");
    return 1;
}

static int Identity(string[] args)
{
    var store = IdentityStore.Default();
    switch (args)
    {
        case [] or ["show"]:
            ShowIdentity(store.Load());
            Console.WriteLine($"\n파일: {store.Path}");
            return 0;
        case ["show", "--json"] or ["--json"]:
            return Print(JsonSerializer.Serialize(store.Load(), Json.Options));
        case ["path"]:
            return Print(store.Path);
        case ["remember", .. var rest] when rest.Length > 0:
        {
            var tags = new List<string>();
            var words = new List<string>();
            for (var i = 0; i < rest.Length; i++)
            {
                if (rest[i] == "--tag" && i + 1 < rest.Length) tags.Add(rest[++i]);
                else words.Add(rest[i]);
            }
            var memory = store.Remember(string.Join(' ', words), tags);
            return Print($"기억했습니다: {memory.Id}");
        }
        case ["forget", var id]:
            return store.Forget(id) ? Print($"삭제했습니다: {id}") : Fail($"기억 {id} 을(를) 찾지 못했습니다.");
        case ["owner", .. var name] when name.Length > 0:
        {
            var profile = store.Load();
            profile.Owner.Name = string.Join(' ', name);
            store.Save(profile);
            return Print($"사용자 이름을 '{profile.Owner.Name}'(으)로 저장했습니다.");
        }
        default:
            return Fail("identity 명령 형식이 올바르지 않습니다. --help 를 참고하세요.");
    }
}

static void ShowIdentity(IdentityProfile p)
{
    Console.WriteLine($"{p.Name} ({p.NameKo}) — 호출명 {p.Callsign}");
    Console.WriteLine(p.Persona.Description);
    Console.WriteLine($"말투: {p.Persona.Tone} · 언어: {p.Persona.Language}");
    if (p.Owner.Name is { } owner) Console.WriteLine($"사용자: {owner}");
    Console.WriteLine("\n원칙:");
    foreach (var principle in p.Principles) Console.WriteLine($"  - {principle}");
    Console.WriteLine($"\n기억 ({p.Memories.Count}개):");
    foreach (var m in p.Memories)
    {
        var when = DateTimeOffset.FromUnixTimeSeconds(m.CreatedAt).ToLocalTime().ToString("yyyy-MM-dd HH:mm");
        var tags = m.Tags.Count > 0 ? $"  #{string.Join(" #", m.Tags)}" : "";
        Console.WriteLine($"  {m.Id}  [{when}] {m.Text}{tags}");
    }
}

static int Net(string[] args)
{
    var report = NetworkProbe.Scan(probe: !args.Contains("--no-probe"));
    if (args.Contains("--json")) return Print(JsonSerializer.Serialize(report, Json.Options));

    Console.WriteLine($"온라인: {(report.Online ? "예" : "아니오")}  도달 가능: {string.Join(", ", report.Reachable)}");
    foreach (var i in report.Interfaces)
        Console.WriteLine($"  {i.Name,-14} {i.Kind,-10} {(i.Up ? "up" : "down"),-5} {string.Join(' ', i.Addresses)}");
    if (report.Wifi is { } w)
        Console.WriteLine($"Wi-Fi: \"{w.Ssid}\"  보안 {w.Security}  신호 {(w.Signal is { } s ? $"{s}%" : "?")}");
    foreach (var warning in report.Warnings) Console.WriteLine($"  ⚠ {warning}");
    return 0;
}
