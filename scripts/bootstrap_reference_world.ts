#!/usr/bin/env bun

type Options = {
  host: string;
  port: number;
  mode: "measure" | "hold";
  timeoutMs: number;
};

function parseArgs(argv: string[]): Options {
  const options: Options = {
    host: "127.0.0.1",
    port: 4178,
    mode: "hold",
    timeoutMs: 60_000,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--host":
        options.host = argv[index + 1] ?? options.host;
        index += 1;
        break;
      case "--port": {
        const value = Number(argv[index + 1]);
        if (!Number.isFinite(value)) {
          throw new Error("missing numeric value for --port");
        }
        options.port = value;
        index += 1;
        break;
      }
      case "--timeout-ms": {
        const value = Number(argv[index + 1]);
        if (!Number.isFinite(value)) {
          throw new Error("missing numeric value for --timeout-ms");
        }
        options.timeoutMs = value;
        index += 1;
        break;
      }
      case "--measure":
        options.mode = "measure";
        break;
      case "--hold":
        options.mode = "hold";
        break;
      case "--help":
      case "-h":
        printHelp();
        process.exit(0);
      default:
        throw new Error(`unknown argument: ${current}`);
    }
  }

  return options;
}

function printHelp() {
  console.error(
    "Usage: bun ./scripts/bootstrap_reference_world.ts [--measure|--hold] [--host 127.0.0.1] [--port 4178] [--timeout-ms 60000]",
  );
}

async function waitForUrl(url: string, timeoutMs: number): Promise<void> {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        return;
      }
    } catch {
      // Server is still booting.
    }
    await Bun.sleep(250);
  }

  throw new Error(`timed out waiting for ${url}`);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const workingDirectory = new URL("../apps/pod-web", import.meta.url).pathname;
  const bootstrapUrl = `http://${options.host}:${options.port}/?world=bootstrap-showcase&backend=webgl2`;

  const server = Bun.spawn(
    [
      "bun",
      "run",
      "dev",
      "--host",
      options.host,
      "--port",
      String(options.port),
    ],
    {
      cwd: workingDirectory,
      env: process.env,
      stdout: options.mode === "hold" ? "inherit" : "pipe",
      stderr: options.mode === "hold" ? "inherit" : "pipe",
    },
  );

  const started = performance.now();
  let shuttingDown = false;
  const shutdown = async () => {
    if (shuttingDown) {
      return;
    }
    shuttingDown = true;
    server.kill();
    await server.exited;
  };

  process.on("SIGINT", async () => {
    await shutdown();
    process.exit(0);
  });
  process.on("SIGTERM", async () => {
    await shutdown();
    process.exit(0);
  });

  try {
    await waitForUrl(bootstrapUrl, options.timeoutMs);
    const startupTimeMs = performance.now() - started;
    const report = {
      mode: options.mode,
      startupTimeMs,
      url: bootstrapUrl,
      workingDirectory,
      notes: [
        "This is the canonical first-world bootstrap for Prompt or Die.",
        "The route boots the authored bootstrap showcase with a controllable human and autonomous agents in one world.",
      ],
    };

    console.log(JSON.stringify(report, null, 2));

    if (options.mode === "measure") {
      await shutdown();
      return;
    }

    console.error(
      `Prompt or Die bootstrap ready at ${bootstrapUrl}. Press Ctrl+C to stop the dev server.`,
    );
    await server.exited;
  } finally {
    if (options.mode === "measure") {
      await shutdown();
    }
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
