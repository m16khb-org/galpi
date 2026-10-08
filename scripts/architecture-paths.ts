/** True when `path` lies inside `directory`, accepting `/` or `\` separators. */
export function isInsideDirectory(path: string, directory: string): boolean {
  const normalize = (value: string): string => value.replaceAll("\\", "/").replace(/\/+$/, "")
  return normalize(path).startsWith(`${normalize(directory)}/`)
}
