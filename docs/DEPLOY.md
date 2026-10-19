# Putting the game online

Two pieces go online, and they are independent:

| Piece | What it is | Where | Cost |
|---|---|---|---|
| **The game** | static files in `web/` (HTML, JS, the `.wasm`) | GitHub Pages | free |
| **The relay** | `server` crate: rooms by code, forwards bytes | Render (free) or Fly.io | free / ~$2 a month |

The game is told where the relay lives by one build-time setting, `RELAY_URL`
(`game/src/net.rs`). Without it the game uses `ws://127.0.0.1:9001/ws`, which
only works on your own computer.

Order matters: relay first (you need its address), then the game.

## 1. Put the code on GitHub

The repository has commits but no remote yet. Create an empty repository on
GitHub, then:

```
git remote add origin https://github.com/<you>/<repo>.git
git push -u origin main
```

(The workflow in `.github/workflows/deploy.yml` runs on pushes to `main`. It
will fail until steps 2 and 3 are done; that is expected.)

## 2. The relay on Render (free)

1. Render dashboard → **New → Blueprint** → choose the repository.
   `render.yaml` describes the service; accept it.
2. The first build compiles Rust from scratch and takes several minutes.
3. When it is live, open `https://<name>.onrender.com/`. It should say
   `neon bomber relay`.
4. The relay's WebSocket address is the same host with `wss://` and `/ws`:
   `wss://<name>.onrender.com/ws`.

**Region matters.** Every message goes host → relay → joiner, so the ping is
the sum of both players' distance to the relay. `render.yaml` sets
`region: singapore`; change it to the region nearest your players *before*
creating the service (Render cannot move an existing one). Check the result
with `curl -o NUL -s -w "%{time_connect}\n" https://<name>.onrender.com/` (one
round trip to the relay, in seconds).

What to expect from the free plan (checked against Render's docs, October 2026):

* The service **falls asleep after 15 minutes without incoming traffic** and
  takes about a minute to wake on the next connection. The game shows
  "Connecting to the relay..." meanwhile. A host sends a small ping every 20 s,
  so the relay stays awake while someone is hosting.
* 750 free instance hours per month: enough for one service running all month.
* Render may restart the service at any time. Open rooms are lost when it does.

### Alternative: Fly.io

`fly.toml` is ready. Fly has no free tier for new accounts (the smallest machine
is about $2 a month) but it has no cold start. From the repo root:

```
fly launch --no-deploy --copy-config
fly deploy --ha=false
```

`--ha=false` matters: rooms live in one server's memory, so two machines would
be two separate worlds.

## 3. The game on GitHub Pages

1. In the GitHub repository: **Settings → Secrets and variables → Actions →
   Variables → New repository variable**:
   `RELAY_URL` = `wss://<name>.onrender.com/ws`
2. **Settings → Pages → Source: GitHub Actions**.
3. Push to `main` (or re-run the workflow). When it finishes, the game is at
   `https://<you>.github.io/<repo>/`.

Browsers only allow `wss://` (not `ws://`) from an `https://` page, which is why
the relay address must start with `wss://`. Render provides the certificate.

## 4. Try it

Open the Pages address in two browsers (or two computers). In one press **H**
and note the room code; in the other press **J** and type it. Remember: the
hosting tab must stay visible.

## Building locally against the deployed relay

```
$env:RELAY_URL = "wss://<name>.onrender.com/ws"
./build_web.ps1
python -m http.server -d web 8000
```

Changing `RELAY_URL` rebuilds the game automatically. The native build cannot
use a `wss://` relay (no TLS support); use it with a local relay only.

## Checking the relay by hand

* `curl https://<name>.onrender.com/` → `neon bomber relay`
* Run it locally: `cargo run -p server --release` (port 9001, or `$PORT`).
* The Dockerfile has **not** been built on the machine that wrote it (no Docker
  there). The exact `cargo build` command inside it was run and works. If the
  first Render build fails, the build log will say why; the usual suspects are a
  Rust or base-image version.
