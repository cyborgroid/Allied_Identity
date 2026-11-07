using System.Text.Json;

namespace AlliedID.Identity;

/// <summary>identity.json 읽기/쓰기와 기억 관리.</summary>
public sealed class IdentityStore(string path)
{
    public string Path { get; } = path;

    public static IdentityStore Default() => new(Paths.IdentityFile);

    public static IdentityProfile BuiltIn()
    {
        using var stream = typeof(IdentityStore).Assembly.GetManifestResourceStream("identity.default.json")
            ?? throw new InvalidOperationException("내장 identity.default.json 을 찾을 수 없습니다.");
        return JsonSerializer.Deserialize<IdentityProfile>(stream, Json.Options)
            ?? throw new InvalidOperationException("내장 identity.default.json 이 비어 있습니다.");
    }

    public IdentityProfile Load()
    {
        if (!File.Exists(Path)) return BuiltIn();
        var text = File.ReadAllText(Path);
        return JsonSerializer.Deserialize<IdentityProfile>(text, Json.Options)
            ?? throw new InvalidDataException($"{Path}: 비어 있는 프로필");
    }

    /// <summary>임시 파일에 쓴 뒤 교체해 중간에 끊겨도 기존 파일이 깨지지 않게 한다.</summary>
    public void Save(IdentityProfile profile)
    {
        Directory.CreateDirectory(System.IO.Path.GetDirectoryName(System.IO.Path.GetFullPath(Path))!);
        var tmp = Path + ".tmp";
        File.WriteAllText(tmp, JsonSerializer.Serialize(profile, Json.Options));
        File.Move(tmp, Path, overwrite: true);
    }

    public Memory Remember(string text, IEnumerable<string>? tags = null)
    {
        var profile = Load();
        var now = DateTimeOffset.UtcNow;
        var memory = new Memory
        {
            Id = $"m{now.ToUnixTimeMilliseconds() * 1_000_000 + Random.Shared.Next(1_000_000):x}",
            Text = text.Trim(),
            CreatedAt = now.ToUnixTimeSeconds(),
            Tags = tags?.ToList() ?? [],
        };
        profile.Memories.Add(memory);
        Save(profile);
        return memory;
    }

    public bool Forget(string id)
    {
        var profile = Load();
        var removed = profile.Memories.RemoveAll(m => m.Id == id) > 0;
        if (removed) Save(profile);
        return removed;
    }
}
