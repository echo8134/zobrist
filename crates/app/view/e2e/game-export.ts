import { once } from "node:events";
import { createServer } from "node:http";
import { text } from "node:stream/consumers";

const portVariable = "ZOBRIST_TEST_EXPORT_PORT";

type PlayerReply = "games" | "empty" | "refused" | "hold";

const playerGames = [
  {
    createdAt: Date.parse("2026-02-01T00:00:00.000Z"),
    pgn: '[Event "February 1"]\n[Date "2026.02.01"]\n[White "RookExample"]\n[Black "Cedar"]\n[Result "1-0"]\n\n1. c4 e5 1-0',
  },
  {
    createdAt: Date.parse("2026-01-31T23:59:59.999Z"),
    pgn: '[Event "January 31"]\n[Date "2026.01.31"]\n[White "Birch"]\n[Black "RookExample"]\n[Result "0-1"]\n\n1. d4 d5 0-1',
  },
  {
    createdAt: Date.parse("2026-01-01T00:00:00.000Z"),
    pgn: '[Event "January 1"]\n[Date "2026.01.01"]\n[White "RookExample"]\n[Black "Aster"]\n[Result "1-0"]\n\n1. e4 e5 1-0',
  },
];

/** The launcher serves exports to the app. The spec chooses the next reply over loopback. */
export async function startGameExports() {
  let status = 503;
  let pgn = "";
  let playerReply = "refused";
  let releaseTablebase: ((status: number) => void) | undefined;
  const requests: string[] = [];
  const server = createServer(async (request, response) => {
    if (request.method === "PUT") {
      if (request.url === "/release-tablebase") {
        if (releaseTablebase === undefined) {
          response.writeHead(409).end();
          return;
        }
        releaseTablebase(Number(await text(request)));
        releaseTablebase = undefined;
        response.writeHead(204).end();
        return;
      }
      if (request.url === "/reply") {
        status = Number(request.headers["x-export-status"]);
        pgn = await text(request);
        requests.length = 0;
      } else if (request.url === "/player-reply") {
        const releasing = playerReply === "hold";
        playerReply = await text(request);
        if (releasing) server.emit("player-reply");
        else requests.length = 0;
      } else {
        response.writeHead(404).end();
        return;
      }
      response.writeHead(204).end();
      return;
    }
    if (request.url === "/requests") {
      response.end(requests.join("\n"));
      return;
    }

    const path = request.url ?? "/";
    const url = new URL(path, "http://127.0.0.1");
    requests.push(path);
    if (url.pathname === "/standard") {
      const body = text(request);
      if (
        request.method !== "GET" ||
        request.headers.authorization !== undefined ||
        request.headers.cookie !== undefined ||
        releaseTablebase !== undefined
      ) {
        response.writeHead(400).end();
        return;
      }
      const status = await new Promise<number>((resolve) => {
        releaseTablebase = resolve;
      });
      if ((await body) !== "") {
        response.writeHead(400).end();
        return;
      }
      response.writeHead(status, { "Content-Type": "application/json" }).end(
        JSON.stringify({
          category: "draw",
          moves: [
            { uci: "e8d7", san: "Kxd7", category: "draw" },
            { uci: "e8f8", san: "Kf8", category: "win" },
          ],
        }),
      );
    } else if (
      request.method === "GET" &&
      request.url === "/game/export/Ab12Cd34" &&
      request.headers.accept === "application/x-chess-pgn"
    ) {
      response
        .writeHead(status, { "Content-Type": "application/x-chess-pgn" })
        .end(pgn);
    } else if (
      request.method === "GET" &&
      url.pathname === "/api/games/user/RookExample" &&
      request.headers.accept === "application/x-ndjson" &&
      url.searchParams.get("pgnInJson") === "true"
    ) {
      if (playerReply === "hold") await once(server, "player-reply");
      if (playerReply === "refused") {
        response.writeHead(404).end();
        return;
      }
      response.writeHead(200, { "Content-Type": "application/x-ndjson" });
      if (playerReply === "games") {
        const since = Number(url.searchParams.get("since"));
        const until = Number(url.searchParams.get("until"));
        for (const game of playerGames) {
          if (game.createdAt >= since && game.createdAt < until) {
            response.write(`${JSON.stringify(game)}\n`);
          }
        }
      }
      response.end();
    } else {
      response.writeHead(400).end();
    }
  });
  // A CLI built without fixtures reaches this listener as its HTTPS proxy.
  server.on("connect", (_request, socket) => socket.destroy());
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("The game export fixture needs a TCP port.");
  }
  process.env[portVariable] = String(address.port);
  return server.unref();
}

export const exportOrigin = () =>
  new URL(`http://127.0.0.1:${process.env[portVariable]}`);

async function setReply(
  path: string,
  body: string,
  headers: Record<string, string> = {},
) {
  const reply = await fetch(new URL(path, exportOrigin()), {
    method: "PUT",
    headers,
    body,
  });
  if (!reply.ok) {
    throw new Error("Could not set the game export fixture.");
  }
}

export function replyWith(status: number, pgn = "") {
  return setReply("/reply", pgn, { "x-export-status": String(status) });
}

export function replyWithPlayerGames(reply: PlayerReply) {
  return setReply("/player-reply", reply);
}

export function releaseTablebase(status: 200 | 503) {
  return setReply("/release-tablebase", String(status));
}

export async function exportPaths(): Promise<string[]> {
  const response = await fetch(new URL("/requests", exportOrigin()));
  const paths = await response.text();
  return paths === "" ? [] : paths.split("\n");
}

export async function exportRequests() {
  return (await exportPaths()).length;
}
