namespace AlliedID.Identity;

/// <summary>Rust 코어(core/src/paths.rs)와 같은 저장 위치 규칙.</summary>
public static class Paths
{
    static string Home =>
        Environment.GetEnvironmentVariable("HOME")
        ?? Environment.GetEnvironmentVariable("USERPROFILE")
        ?? ".";

    static string? Override =>
        Environment.GetEnvironmentVariable("ALLIEDID_HOME") is { Length: > 0 } dir ? dir : null;

    static string Env(string name, string fallback) =>
        Environment.GetEnvironmentVariable(name) is { Length: > 0 } v ? v : fallback;

    public static string ConfigDir => Override ?? (
        OperatingSystem.IsWindows() ? Path.Combine(Env("APPDATA", Home), "AlliedID")
        : OperatingSystem.IsMacOS() ? Path.Combine(Home, "Library", "Application Support", "AlliedID")
        : Path.Combine(Env("XDG_CONFIG_HOME", Path.Combine(Home, ".config")), "allied-id"));

    public static string DataDir => Override ?? (
        OperatingSystem.IsWindows() ? Path.Combine(Env("LOCALAPPDATA", Home), "AlliedID")
        : OperatingSystem.IsMacOS() ? Path.Combine(Home, "Library", "Application Support", "AlliedID")
        : Path.Combine(Env("XDG_DATA_HOME", Path.Combine(Home, ".local", "share")), "allied-id"));

    public static string IdentityFile => Path.Combine(ConfigDir, "identity.json");
    public static string KnowledgeDir => Path.Combine(DataDir, "knowledge");
}
