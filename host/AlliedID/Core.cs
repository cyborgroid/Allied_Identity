using System.Runtime.InteropServices;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace AlliedID;

/// <summary>Rust 본체(allied_core)의 C ABI.</summary>
internal static partial class Native
{
    const string Lib = "allied_core";

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr allied_engine_new(string config);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr allied_engine_call(IntPtr engine, string request);

    [LibraryImport(Lib)]
    internal static partial void allied_engine_free(IntPtr engine);

    [LibraryImport(Lib)]
    internal static partial void allied_string_free(IntPtr s);

    [LibraryImport(Lib)]
    internal static partial IntPtr allied_last_error();

    [LibraryImport(Lib)]
    internal static partial IntPtr allied_version();
}

public sealed class CoreException(string message) : Exception(message);

/// <summary>본체 엔진 핸들. 요청·응답은 모두 JSON.</summary>
public sealed class CoreEngine : IDisposable
{
    IntPtr _handle;

    public CoreEngine(object config)
    {
        _handle = Native.allied_engine_new(JsonSerializer.Serialize(config, Json.Options));
        if (_handle == IntPtr.Zero)
            throw new CoreException(Take(Native.allied_last_error()) ?? "본체를 시작하지 못했습니다.");
    }

    public static string Version => Take(Native.allied_version()) ?? "?";

    public JsonElement Call(object request)
    {
        ObjectDisposedException.ThrowIf(_handle == IntPtr.Zero, this);
        var reply = Take(Native.allied_engine_call(_handle, JsonSerializer.Serialize(request, Json.Options)))
            ?? throw new CoreException("본체가 응답하지 않았습니다.");
        using var doc = JsonDocument.Parse(reply);
        var root = doc.RootElement;
        if (!root.GetProperty("ok").GetBoolean())
            throw new CoreException(root.GetProperty("error").GetString() ?? "알 수 없는 오류");
        return root.GetProperty("result").Clone();
    }

    static string? Take(IntPtr p)
    {
        if (p == IntPtr.Zero) return null;
        try { return Marshal.PtrToStringUTF8(p); }
        finally { Native.allied_string_free(p); }
    }

    public void Dispose()
    {
        if (_handle != IntPtr.Zero) Native.allied_engine_free(_handle);
        _handle = IntPtr.Zero;
    }
}

public static class Json
{
    /// <summary>Rust 쪽 serde 와 같은 snake_case 이름.</summary>
    public static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        WriteIndented = false,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        DefaultIgnoreCondition = JsonIgnoreCondition.Never,
    };

    public static readonly JsonSerializerOptions Pretty = new(Options) { WriteIndented = true };
}

/// <summary>프로젝트 안의 위치. 데이터는 모두 프로젝트의 data/ 아래에 있다.</summary>
public static class ProjectPaths
{
    public static readonly string Root = FindRoot();
    public static string Data => Path.Combine(Root, "data");
    public static string Seed => Path.Combine(Data, "seed");
    public static string State => Path.Combine(Data, "state");
    public static string Generated => Path.Combine(Data, "generated");

    static string FindRoot()
    {
        if (Environment.GetEnvironmentVariable("ALLIEDID_ROOT") is { Length: > 0 } env && Directory.Exists(Path.Combine(env, "data", "seed")))
            return Path.GetFullPath(env);
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir is not null; dir = dir.Parent)
            if (Directory.Exists(Path.Combine(dir.FullName, "data", "seed")))
                return dir.FullName;
        throw new DirectoryNotFoundException("프로젝트 폴더(data/seed)를 찾지 못했습니다. ALLIEDID_ROOT 를 지정하세요.");
    }
}
