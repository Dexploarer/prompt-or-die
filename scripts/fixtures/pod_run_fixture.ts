const [mode, ...rest] = Bun.argv.slice(2);

async function main() {
  switch (mode) {
    case "args": {
      console.log(JSON.stringify(rest));
      if (process.env.POD_TEST_VALUE) {
        console.error(`env:${process.env.POD_TEST_VALUE}`);
      }
      break;
    }
    case "fail": {
      console.error("fixture failed");
      process.exit(7);
    }
    case "repeat": {
      const count = Number(rest[0] ?? "4096");
      console.log("x".repeat(count));
      console.error("y".repeat(count));
      break;
    }
    case "stream": {
      const delayMs = Number(rest[0] ?? "10");
      console.log("stream:stdout:1");
      console.error("stream:stderr:1");
      await Bun.sleep(delayMs);
      console.log("stream:stdout:2");
      console.error("stream:stderr:2");
      await Bun.sleep(delayMs);
      break;
    }
    case "delayed-exit": {
      const delayMs = Number(rest[0] ?? "10");
      const exitCode = Number(rest[1] ?? "0");
      console.log(`delayed-exit:stdout:${exitCode}`);
      console.error(`delayed-exit:stderr:${exitCode}`);
      await Bun.sleep(delayMs);
      process.exit(exitCode);
    }
    default: {
      console.error(`unknown mode:${mode ?? "<missing>"}`);
      process.exit(64);
    }
  }
}

await main();
