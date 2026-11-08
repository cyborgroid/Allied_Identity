using System.Diagnostics;
using System.Text;
using System.Text.Json;
using AlliedID;

// AlliedID — Allied Identity (고유 인격체 파트너)
// 본체는 Rust 엔진(allied_core), 이 C# 호스트는 몸: 터미널 대화, 인격체·기억 저장, 컴퓨터·네트워크 관찰.

Console.OutputEncoding = Encoding.UTF8;
Console.InputEncoding = Encoding.UTF8;

const string Help = """
AlliedID — Allied Identity (고유 인격체 파트너)

  AlliedID                 대화 시작
  AlliedID ask <질문>      한 번 묻고 끝내기
  AlliedID status          본체·데이터 상태
  AlliedID net             네트워크 환경
  AlliedID forge           데이터를 다시 만들고 결과 보기

대화 명령:
  /think                 생각 과정 보기 켜기·끄기
  /effort low|medium|high  노력 수준
  /teach 질문 => 답      가르치기
  /good  /bad            직전 답 평가 (맞게 이해했으면 /good → 배움)
  /memories              기억 목록      /forget <id>  기억 지우기
  /forge                 데이터 다시 만들기
  /status  /clear  /help  /exit
""";

bool color = !Console.IsOutputRedirected;
string Dim(string s) => color ? $"\e[2m{s}\e[0m" : s;
string Accent(string s) => color ? $"\e[36m{s}\e[0m" : s;

try
{
    return Run(args);
}
catch (Exception e) when (e is CoreException or DllNotFoundException or DirectoryNotFoundException or IOException or InvalidDataException)
{
    Console.Error.WriteLine($"오류: {e.Message}");
    return 1;
}

int Run(string[] args)
{
    switch (args)
    {
        case ["-h" or "--help" or "help"]:
            Console.Write(Help);
            return 0;
        case ["-V" or "--version" or "version"]:
            Console.WriteLine($"AlliedID {CoreEngine.Version}");
            return 0;
        case ["net"]:
            PrintNet(NetworkProbe.Scan());
            return 0;
    }

    var store = new IdentityStore();
    using var core = new CoreEngine(new { data_dir = ProjectPaths.Data, effort = "medium", identity = store.Load() });
    var tools = new HostTools(store);
    var (net, forged, elapsed) = Forge(core);

    switch (args)
    {
        case []:
            return Repl(core, store, tools, net, forged, elapsed);
        case ["ask", .. var q] when q.Length > 0:
            Console.WriteLine(Converse(core, tools, string.Join(' ', q), showThinking: false));
            return 0;
        case ["status"]:
            PrintStatus(core, store);
            return 0;
        case ["forge"]:
            Console.WriteLine($"데이터 생성 완료 ({elapsed.TotalMilliseconds:F0}ms) → {ProjectPaths.Generated}");
            Console.WriteLine(JsonSerializer.Serialize(forged, Json.Pretty));
            return 0;
        default:
            Console.Error.Write(Help);
            return 2;
    }
}

/// 프로젝트 환경을 관찰해 본체가 데이터를 새로 만들게 한다.
(NetReport, JsonElement, TimeSpan) Forge(CoreEngine core)
{
    var sw = Stopwatch.StartNew();
    var net = NetworkProbe.Scan();
    var stats = core.Call(new { type = "forge", environment = EnvironmentCollector.Collect(net) });
    return (net, stats, sw.Elapsed);
}

/// Claude 식 도구 루프: 본체가 tool_use 로 멈추면 호스트가 실행하고 tool_result 로 돌려준다.
string Converse(CoreEngine core, HostTools tools, string text, bool showThinking)
{
    var r = core.Call(new { type = "message", text });
    for (var step = 0; step < 8; step++)
    {
        var blocks = r.GetProperty("content").EnumerateArray().ToList();
        if (showThinking)
            foreach (var b in blocks.Where(b => b.GetProperty("type").GetString() == "thinking"))
                Console.WriteLine(Dim("  ⋯ " + b.GetProperty("thinking").GetString()!.Replace("\n", "\n  ⋯ ")));

        if (r.GetProperty("stop_reason").GetString() != "tool_use")
            return string.Join("\n", blocks.Where(b => b.GetProperty("type").GetString() == "text").Select(b => b.GetProperty("text").GetString()));

        var use = blocks.First(b => b.GetProperty("type").GetString() == "tool_use");
        var name = use.GetProperty("name").GetString()!;
        if (showThinking) Console.WriteLine(Dim($"  ⚙ {name} 실행"));
        var (content, isError) = tools.Run(name, use.GetProperty("input"));
        r = core.Call(new { type = "tool_result", tool_use_id = use.GetProperty("id").GetString(), content, is_error = isError });
    }
    return "생각이 너무 길어져 멈췄어요.";
}

int Repl(CoreEngine core, IdentityStore store, HostTools tools, NetReport net, JsonElement forged, TimeSpan elapsed)
{
    var id = store.Load();
    var showThinking = false;
    Console.WriteLine(Accent($"{id.Name} ({id.NameKo})") + Dim($" · AlliedID {CoreEngine.Version}"));
    Console.WriteLine(Dim($"데이터 생성: 지식 {forged.GetProperty("knowledge_docs")} · 학습 예문 {forged.GetProperty("training_examples")} · " +
                          $"가르침 {forged.GetProperty("learned_docs")} ({elapsed.TotalSeconds:F1}초) · 네트워크 {(net.Online ? "온라인" : "오프라인")}"));
    foreach (var w in net.Warnings) Console.WriteLine($"  ⚠ {w}");
    Console.WriteLine(Dim("/help 로 명령 보기 · /exit 로 끝내기"));

    while (true)
    {
        Console.Write("\n" + Accent("> "));
        var line = Console.ReadLine();
        if (line is null) { Console.WriteLine(); return 0; }
        line = line.Trim();
        if (line.Length == 0) continue;

        if (!line.StartsWith('/'))
        {
            Console.WriteLine();
            Console.WriteLine(Converse(core, tools, line, showThinking));
            continue;
        }

        var parts = line[1..].Split(' ', 2, StringSplitOptions.TrimEntries);
        var arg = parts.Length > 1 ? parts[1] : "";
        switch (parts[0])
        {
            case "exit" or "quit" or "q":
                return 0;
            case "help":
                Console.Write(Help);
                break;
            case "think":
                showThinking = !showThinking;
                Console.WriteLine($"생각 과정 보기: {(showThinking ? "켬" : "끔")}");
                break;
            case "effort" when arg is "low" or "medium" or "high":
                core.Call(new { type = "effort", effort = arg });
                Console.WriteLine($"노력 수준: {arg}");
                break;
            case "teach":
                var qa = arg.Split("=>", 2, StringSplitOptions.TrimEntries);
                if (qa.Length != 2 || qa[0].Length == 0 || qa[1].Length == 0)
                    Console.WriteLine("형식: /teach 질문 => 답");
                else
                    Console.WriteLine(core.Call(new { type = "teach", question = qa[0], answer = qa[1] }).GetProperty("text").GetString());
                break;
            case "good" or "bad":
                Console.WriteLine(core.Call(new { type = "feedback", good = parts[0] == "good" }).GetProperty("text").GetString());
                break;
            case "memories":
                var p = store.Load();
                if (p.Owner.Name is { } owner) Console.WriteLine($"사용자: {owner}");
                if (p.Memories.Count == 0) Console.WriteLine("아직 기억이 없어요.");
                foreach (var m in p.Memories)
                    Console.WriteLine($"  {m.Id}  {DateTimeOffset.FromUnixTimeSeconds(m.CreatedAt).ToLocalTime():yyyy-MM-dd}  {m.Text}");
                break;
            case "forget" when arg.Length > 0:
                var removed = store.Forget(arg);
                core.Call(new { type = "identity", identity = store.Load() });
                Console.WriteLine(removed.Count > 0 ? $"기억 {removed.Count}개를 지웠어요." : "그런 기억이 없어요.");
                break;
            case "forge":
                var (n, f, t) = Forge(core);
                Console.WriteLine($"데이터를 다시 만들었어요: 지식 {f.GetProperty("knowledge_docs")} · 학습 예문 {f.GetProperty("training_examples")} ({t.TotalSeconds:F1}초, {(n.Online ? "온라인" : "오프라인")})");
                break;
            case "status":
                PrintStatus(core, store);
                break;
            case "clear":
                core.Call(new { type = "reset" });
                Console.WriteLine("대화 맥락을 비웠어요. (기억과 배운 것은 그대로예요)");
                break;
            default:
                Console.WriteLine("알 수 없는 명령이에요. /help 를 입력하세요.");
                break;
        }
    }
}

void PrintStatus(CoreEngine core, IdentityStore store)
{
    var s = core.Call(new { type = "status" });
    var id = store.Load();
    Console.WriteLine($"{id.Name} ({id.NameKo}) · 본체 {s.GetProperty("version")}");
    Console.WriteLine($"  사용자      {id.Owner.Name ?? "(모름)"}");
    Console.WriteLine($"  지식 조각   {s.GetProperty("knowledge_docs")}");
    Console.WriteLine($"  학습 예문   {s.GetProperty("training_examples")}  (의도 {s.GetProperty("intents")}개, 어휘 {s.GetProperty("vocabulary")})");
    Console.WriteLine($"  가르침      {s.GetProperty("learned")}   확인된 표현 {s.GetProperty("confirmed")}   기억 {s.GetProperty("memories")}");
    Console.WriteLine($"  노력 수준   {s.GetProperty("effort")}");
    Console.WriteLine($"  데이터      {ProjectPaths.Data}");
}

void PrintNet(NetReport r)
{
    Console.WriteLine($"인터넷: {(r.Online ? "닿음" : "닿지 않음")}  {string.Join(", ", r.Reachable)}");
    foreach (var i in r.Interfaces)
        Console.WriteLine($"  {i.Name,-16} {i.Kind,-10} {(i.Up ? "up" : "down"),-5} {string.Join(' ', i.Addresses)}");
    if (r.Wifi is { } w) Console.WriteLine($"Wi-Fi: \"{w.Ssid}\"  보안 {w.Security}  신호 {(w.Signal is { } s ? $"{s}%" : "?")}");
    foreach (var m in r.Cellular) Console.WriteLine($"셀룰러 모뎀: {m}");
    foreach (var warn in r.Warnings) Console.WriteLine($"  ⚠ {warn}");
}
