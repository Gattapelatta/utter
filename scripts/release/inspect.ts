// Reading what `codesign -dv` and `lipo -archs` say about a built app.

/** `codesign -dv` writes to stderr; an ad-hoc signature has no Authority line. */
export function parseCodesign(out: string): { authority: string | null; identifier: string | null } {
  const field = (name: string) => out.match(new RegExp(`^${name}=(.+)$`, "m"))?.[1].trim() ?? null;
  return { authority: field("Authority"), identifier: field("Identifier") };
}

export function parseLipo(out: string): string[] {
  return out.trim().split(/\s+/).filter(Boolean);
}
