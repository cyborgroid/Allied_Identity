using System.Text.Json;

namespace AlliedID;

public sealed class IdentityProfile
{
    public int SchemaVersion { get; set; } = 2;
    public string Name { get; set; } = "";
    public string NameKo { get; set; } = "";
    public string Callsign { get; set; } = "";
    public Persona Persona { get; set; } = new();
    public List<string> Principles { get; set; } = [];
    public Owner Owner { get; set; } = new();
    public List<Memory> Memories { get; set; } = [];
}

public sealed class Persona
{
    public string Description { get; set; } = "";
    public string Tone { get; set; } = "";
    public string Language { get; set; } = "ko";
}

public sealed class Owner
{
    public string? Name { get; set; }
    public string? Notes { get; set; }
}

public sealed class Memory
{
    public required string Id { get; set; }
    public required string Text { get; set; }
    public long CreatedAt { get; set; }
    public List<string> Tags { get; set; } = [];
}

/// <summary>인격체 파일(data/state/identity.json). 없으면 씨앗(data/seed/identity.default.json)에서 시작한다.</summary>
public sealed class IdentityStore
{
    public string Path { get; } = System.IO.Path.Combine(ProjectPaths.State, "identity.json");
    string SeedPath => System.IO.Path.Combine(ProjectPaths.Seed, "identity.default.json");

    public IdentityProfile Load()
    {
        var file = File.Exists(Path) ? Path : SeedPath;
        return JsonSerializer.Deserialize<IdentityProfile>(File.ReadAllText(file), Json.Options)
            ?? throw new InvalidDataException($"{file}: 비어 있는 인격체 파일");
    }

    public void Save(IdentityProfile profile)
    {
        Directory.CreateDirectory(ProjectPaths.State);
        var tmp = Path + ".tmp";
        File.WriteAllText(tmp, JsonSerializer.Serialize(profile, Json.Pretty));
        File.Move(tmp, Path, overwrite: true);
    }

    public Memory Remember(string text)
    {
        var p = Load();
        var now = DateTimeOffset.UtcNow;
        var m = new Memory
        {
            Id = $"m{now.ToUnixTimeMilliseconds():x}{Random.Shared.Next(0x1000):x3}",
            Text = text.Trim(),
            CreatedAt = now.ToUnixTimeSeconds(),
        };
        p.Memories.Add(m);
        Save(p);
        return m;
    }

    /// <summary>질의어가 모두 들어 있는 기억을 지운다. id 를 그대로 줘도 된다.</summary>
    public List<Memory> Forget(string query)
    {
        var p = Load();
        var words = query.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        var removed = p.Memories.Where(m => m.Id == query || (words.Length > 0 && words.All(w => m.Text.Contains(w, StringComparison.OrdinalIgnoreCase)))).ToList();
        if (removed.Count > 0)
        {
            p.Memories.RemoveAll(removed.Contains);
            Save(p);
        }
        return removed;
    }

    public void SetOwner(string name)
    {
        var p = Load();
        p.Owner.Name = name;
        Save(p);
    }
}
