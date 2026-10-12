// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using System.Net.WebSockets;
using System.Text;
using System.Text.Json;
using Cairn.Wire;

namespace Cairn.MasterServer.Features.Presence;

/// <summary>The launcher's end of the presence feature: one WebSocket per open launcher (see <see cref="PresenceApi"/>).</summary>
internal static class PresenceEndpoint
{
	private static readonly TimeSpan HelloTimeout = TimeSpan.FromSeconds(30);

	public static void MapPresence(this WebApplication app) => app.Map(PresenceApi.ConnectPath, HandleAsync);

	private static async Task HandleAsync(HttpContext context, PresenceHub hub, ILoggerFactory loggerFactory)
	{
		var log = loggerFactory.CreateLogger("Cairn.MasterServer.Presence");
		if (!context.WebSockets.IsWebSocketRequest)
		{
			context.Response.StatusCode = StatusCodes.Status400BadRequest;
			await context.Response.WriteAsync("This endpoint is a WebSocket. Launchers connect here to see their online friends.");
			return;
		}

		using var socket = await context.WebSockets.AcceptWebSocketAsync();
		var sendGate = new SemaphoreSlim(1, 1);
		async Task Send(PresenceMessage m)
		{
			if (socket.State != WebSocketState.Open) return;
			var bytes = JsonSerializer.SerializeToUtf8Bytes(m, PresenceApi.Json);
			await sendGate.WaitAsync();
			try { await socket.SendAsync(bytes, WebSocketMessageType.Text, true, CancellationToken.None); }
			finally { sendGate.Release(); }
		}
		Task Close() => socket.State == WebSocketState.Open
			? socket.CloseAsync(WebSocketCloseStatus.NormalClosure, "Signed in elsewhere", CancellationToken.None)
			: Task.CompletedTask;

		PresenceHub.Peer? peer = null;
		try
		{
			var hello = await ReceiveAsync(socket, HelloTimeout, context.RequestAborted);
			if (hello is null) return;
			if (hello.ApiVersion != PresenceApi.Version || hello.Type != PresenceApi.MessageType.Hello || hello.Hello is null)
			{
				await Send(new PresenceMessage { Type = PresenceApi.MessageType.Refused, ApiVersion = PresenceApi.Version, Reason = "Speak presence API v" + PresenceApi.Version + ", starting with a hello." });
				return;
			}
			var (accepted, refused) = await hub.HelloAsync(hello.Hello, Send, Close);
			if (accepted is null)
			{
				await Send(new PresenceMessage { Type = PresenceApi.MessageType.Refused, ApiVersion = PresenceApi.Version, Reason = refused });
				return;
			}
			peer = accepted;
			log.LogInformation("{Online} online ({Name} joined).", hub.Online, peer.Name);

			var timeout = TimeSpan.FromSeconds(PresenceApi.TimeoutSeconds);
			while (socket.State == WebSocketState.Open)
			{
				var m = await ReceiveAsync(socket, timeout, context.RequestAborted);
				if (m is null) break;
				switch (m.Type)
				{
					case PresenceApi.MessageType.Status when m.Status is not null:
						await hub.StatusAsync(peer, m.Status);
						break;
					case PresenceApi.MessageType.Invite when m.Invite is not null:
						string? why = await hub.InviteAsync(peer, m.Invite, DateTimeOffset.UtcNow);
						if (why is not null) await Send(new PresenceMessage { Type = PresenceApi.MessageType.Refused, ApiVersion = PresenceApi.Version, Reason = why });
						break;
					default:
						break;   // a ping, or something a newer launcher sends that this master has no use for
				}
			}
		}
		catch (OperationCanceledException) { }
		catch (WebSocketException) { }
		catch (JsonException ex) { log.LogWarning("Dropping a presence connection: not a presence message ({Message}).", ex.Message); }
		finally
		{
			if (peer is not null) { await hub.LeaveAsync(peer); log.LogInformation("{Online} online ({Name} left).", hub.Online, peer.Name); }
		}
	}

	private static async Task<PresenceMessage?> ReceiveAsync(WebSocket socket, TimeSpan timeout, CancellationToken ct)
	{
		using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct);
		cts.CancelAfter(timeout);
		var buffer = new byte[4096];
		using var ms = new MemoryStream();
		while (true)
		{
			var r = await socket.ReceiveAsync(buffer, cts.Token);
			if (r.MessageType == WebSocketMessageType.Close) return null;
			ms.Write(buffer, 0, r.Count);
			if (ms.Length > PresenceApi.MaxMessageBytes) throw new JsonException("message too large");
			if (r.EndOfMessage) break;
		}
		return JsonSerializer.Deserialize<PresenceMessage>(Encoding.UTF8.GetString(ms.GetBuffer(), 0, (int)ms.Length), PresenceApi.Json);
	}
}
