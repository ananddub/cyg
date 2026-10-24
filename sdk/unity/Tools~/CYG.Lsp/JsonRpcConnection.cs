using System.Text;
using System.Text.Json;

namespace CookYourGame.Lsp;

/// <summary>Minimal LSP-framed JSON-RPC transport over stdin/stdout.</summary>
internal sealed class JsonRpcConnection
{
    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        DefaultIgnoreCondition = System.Text.Json.Serialization.JsonIgnoreCondition.Never,
    };

    private readonly Stream _input;
    private readonly Stream _output;
    private readonly object _writeLock = new();

    public JsonRpcConnection(Stream input, Stream output)
    {
        _input = input;
        _output = output;
    }

    public JsonDocument? Read()
    {
        int contentLength = -1;
        while (true)
        {
            string? line = ReadHeaderLine();
            if (line == null)
            {
                return null;
            }

            if (line.Length == 0)
            {
                break;
            }

            int colon = line.IndexOf(':');
            if (colon <= 0)
            {
                continue;
            }

            string name = line[..colon].Trim();
            string value = line[(colon + 1)..].Trim();
            if (name.Equals("Content-Length", StringComparison.OrdinalIgnoreCase))
            {
                if (!int.TryParse(value, out contentLength) || contentLength < 0)
                {
                    throw new InvalidDataException("Invalid Content-Length header.");
                }
            }
        }

        if (contentLength < 0)
        {
            throw new InvalidDataException("Missing Content-Length header.");
        }

        if (contentLength == 0)
        {
            return JsonDocument.Parse("null");
        }

        byte[] buffer = new byte[contentLength];
        int read = 0;
        while (read < contentLength)
        {
            int chunk = _input.Read(buffer, read, contentLength - read);
            if (chunk <= 0)
            {
                throw new EndOfStreamException("The LSP stream ended inside a message body.");
            }

            read += chunk;
        }

        return JsonDocument.Parse(buffer);
    }

    public void Send(object message)
    {
        byte[] body = JsonSerializer.SerializeToUtf8Bytes(message, SerializerOptions);
        byte[] header = Encoding.ASCII.GetBytes($"Content-Length: {body.Length}\r\n\r\n");
        lock (_writeLock)
        {
            _output.Write(header, 0, header.Length);
            _output.Write(body, 0, body.Length);
            _output.Flush();
        }
    }

    public void SendResult(JsonElement id, object? result)
    {
        Send(new Dictionary<string, object?>
        {
            ["jsonrpc"] = "2.0",
            ["id"] = id.Clone(),
            ["result"] = result,
        });
    }

    public void SendError(JsonElement id, int code, string message)
    {
        Send(new Dictionary<string, object?>
        {
            ["jsonrpc"] = "2.0",
            ["id"] = id.Clone(),
            ["error"] = new { code, message },
        });
    }

    public void SendNotification(string method, object parameters)
    {
        Send(new Dictionary<string, object?>
        {
            ["jsonrpc"] = "2.0",
            ["method"] = method,
            ["params"] = parameters,
        });
    }

    private string? ReadHeaderLine()
    {
        var bytes = new List<byte>(64);
        while (true)
        {
            int value = _input.ReadByte();
            if (value < 0)
            {
                if (bytes.Count == 0)
                {
                    return null;
                }

                throw new EndOfStreamException("The LSP stream ended inside a header.");
            }

            if (value == '\n')
            {
                break;
            }

            if (value != '\r')
            {
                bytes.Add((byte)value);
            }
        }

        return Encoding.ASCII.GetString(bytes.ToArray());
    }
}
