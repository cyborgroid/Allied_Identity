using System.Text.Json;
using System.Text.Json.Serialization;

namespace AlliedID.Identity;

/// <summary>schemas/identity.schema.json 형식의 인격체 프로필.</summary>
public sealed class IdentityProfile
{
    public int SchemaVersion { get; set; } = 1;
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

public static class Json
{
    /// <summary>Rust 쪽 serde 와 같은 snake_case 필드 이름.</summary>
    public static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        WriteIndented = true,
        Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        DefaultIgnoreCondition = JsonIgnoreCondition.Never,
    };
}
