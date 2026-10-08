export type BuildCommand = readonly string[]

export function bundleCommands(platform: string): readonly BuildCommand[] {
  if (platform === "darwin") {
    return [
      ["cargo", "tauri", "build", "--bundles", "app", "--ci"],
      ["bun", "run", "dmg:build"],
    ]
  }
  if (platform === "win32") {
    return [["cargo", "tauri", "build", "--bundles", "nsis", "--ci"]]
  }
  throw new Error(`unsupported build platform: ${platform} (supported: darwin, win32)`)
}

async function main(): Promise<void> {
  for (const command of bundleCommands(process.platform)) {
    const child = Bun.spawn([...command], { stdout: "inherit", stderr: "inherit" })
    const exitCode = await child.exited
    if (exitCode !== 0) {
      throw new Error(`${command.join(" ")} exited with code ${exitCode}`)
    }
  }
}

if (import.meta.main) {
  await main()
}
