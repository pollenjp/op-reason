// A fake op.exe for the Windows tests, compiled with Add-Type -OutputType ConsoleApplication.
//
// Every call appends one JSON line to %FAKE_DIR%\calls.jsonl: the arguments as received, whether OPREASON_PAYLOAD was
// inherited, OP_SERVICE_ACCOUNT_TOKEN and OP_BIOMETRIC_UNLOCK_ENABLED.
//   whoami   exits with FAKE_WHOAMI_RC (default 1)
//   signout  writes SIGNOUT-OUT / SIGNOUT-ERR and exits 0
//   other    sleeps FAKE_SLEEP_MS, writes FAKE_OUT, copies stdin to stdout byte for byte (unless FAKE_NO_STDIN is set),
//            writes FAKE_ERR_TEXT (default "FAKE-ERR") to stderr and exits with FAKE_RC (default 0)
using System;
using System.IO;
using System.Text;
using System.Threading;

public static class FakeOp
{
    static string Env(string name)
    {
        string v = Environment.GetEnvironmentVariable(name);
        return string.IsNullOrEmpty(v) ? null : v;
    }

    static string Json(string s)
    {
        if (s == null) return "null";
        var sb = new StringBuilder("\"");
        foreach (char c in s)
        {
            if (c == '"') sb.Append("\\\"");
            else if (c == '\\') sb.Append("\\\\");
            else if (c < 0x20 || c > 0x7e) sb.AppendFormat("\\u{0:x4}", (int)c);
            else sb.Append(c);
        }
        return sb.Append('"').ToString();
    }

    public static int Main(string[] args)
    {
        string dir = Env("FAKE_DIR") ?? Path.GetTempPath();
        var line = new StringBuilder("{\"args\":[");
        for (int i = 0; i < args.Length; i++)
        {
            if (i > 0) line.Append(',');
            line.Append(Json(args[i]));
        }
        line.Append("],\"payload\":").Append(Environment.GetEnvironmentVariable("OPREASON_PAYLOAD") != null ? "true" : "false");
        line.Append(",\"sa\":").Append(Json(Environment.GetEnvironmentVariable("OP_SERVICE_ACCOUNT_TOKEN")));
        line.Append(",\"bio\":").Append(Json(Environment.GetEnvironmentVariable("OP_BIOMETRIC_UNLOCK_ENABLED")));
        line.Append("}\n");
        File.AppendAllText(Path.Combine(dir, "calls.jsonl"), line.ToString(), new UTF8Encoding(false));

        string first = args.Length > 0 ? args[0] : "";
        if (first == "whoami") return int.Parse(Env("FAKE_WHOAMI_RC") ?? "1");
        if (first == "signout")
        {
            Console.Out.Write("SIGNOUT-OUT\n");
            Console.Error.Write("SIGNOUT-ERR\n");
            return 0;
        }
        string sleep = Env("FAKE_SLEEP_MS");
        if (sleep != null) Thread.Sleep(int.Parse(sleep));
        using (Stream stdout = Console.OpenStandardOutput())
        {
            string text = Env("FAKE_OUT");
            if (text != null)
            {
                byte[] b = Encoding.UTF8.GetBytes(text);
                stdout.Write(b, 0, b.Length);
            }
            if (Env("FAKE_NO_STDIN") == null)
            {
                using (Stream stdin = Console.OpenStandardInput())
                {
                    stdin.CopyTo(stdout);
                }
            }
            stdout.Flush();
        }
        using (Stream stderr = Console.OpenStandardError())
        {
            byte[] e = Encoding.UTF8.GetBytes((Env("FAKE_ERR_TEXT") ?? "FAKE-ERR") + "\n");
            stderr.Write(e, 0, e.Length);
        }
        return int.Parse(Env("FAKE_RC") ?? "0");
    }
}
