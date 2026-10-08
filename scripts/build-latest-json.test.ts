import { describe, expect, it } from "vitest";
// @ts-expect-error - plain ESM module, no type declarations
import { buildLatestJson } from "./build-latest-json.mjs";

const REPO = "smo-key/ken";

/** The full signed-asset set a green four-leg release produces. */
function fullAssets() {
  return [
    { name: "Ken_aarch64.app.tar.gz.sig", sigText: "sig-darwin-arm\n" },
    { name: "Ken_x64.app.tar.gz.sig", sigText: "sig-darwin-intel\n" },
    { name: "Ken_0.2.0_amd64.AppImage.sig", sigText: "sig-appimage\n" },
    { name: "Ken_0.2.0_amd64.deb.sig", sigText: "sig-deb\n" },
    { name: "Ken-0.2.0-1.x86_64.rpm.sig", sigText: "sig-rpm\n" },
    { name: "Ken_0.2.0_x64-setup.exe.sig", sigText: "sig-nsis\n" },
  ];
}

function build(assets: Array<{ name: string; sigText: string }>) {
  return buildLatestJson({
    version: "0.2.0",
    pubDate: "2026-01-02T03:04:05.000Z",
    notes: "See the release notes.",
    repo: REPO,
    assets,
  });
}

describe("buildLatestJson", () => {
  it("produces every platform key tauri-action emits, with latest/download URLs", () => {
    const manifest = build(fullAssets());

    expect(manifest.version).toBe("0.2.0");
    expect(manifest.pub_date).toBe("2026-01-02T03:04:05.000Z");
    expect(manifest.notes).toBe("See the release notes.");

    expect(Object.keys(manifest.platforms).sort()).toEqual(
      [
        "darwin-aarch64",
        "darwin-aarch64-app",
        "darwin-x86_64",
        "darwin-x86_64-app",
        "linux-x86_64",
        "linux-x86_64-appimage",
        "linux-x86_64-deb",
        "linux-x86_64-rpm",
        "windows-x86_64",
        "windows-x86_64-nsis",
      ].sort(),
    );

    const base = `https://github.com/${REPO}/releases/latest/download`;
    expect(manifest.platforms["darwin-aarch64"]).toEqual({
      signature: "sig-darwin-arm",
      url: `${base}/Ken_aarch64.app.tar.gz`,
    });
    expect(manifest.platforms["darwin-aarch64-app"]).toEqual(
      manifest.platforms["darwin-aarch64"],
    );
    expect(manifest.platforms["darwin-x86_64"].url).toBe(`${base}/Ken_x64.app.tar.gz`);
    expect(manifest.platforms["linux-x86_64"].url).toBe(`${base}/Ken_0.2.0_amd64.AppImage`);
    expect(manifest.platforms["linux-x86_64-appimage"].url).toBe(
      `${base}/Ken_0.2.0_amd64.AppImage`,
    );
    expect(manifest.platforms["linux-x86_64-deb"].url).toBe(`${base}/Ken_0.2.0_amd64.deb`);
    expect(manifest.platforms["linux-x86_64-rpm"].url).toBe(`${base}/Ken-0.2.0-1.x86_64.rpm`);
    expect(manifest.platforms["windows-x86_64"].url).toBe(`${base}/Ken_0.2.0_x64-setup.exe`);
    expect(manifest.platforms["windows-x86_64-nsis"].signature).toBe("sig-nsis");
  });

  it("maps the MSI signature even though the MSI is no longer built", () => {
    const manifest = build([
      ...fullAssets(),
      { name: "Ken_0.2.0_x64_en-US.msi.sig", sigText: "sig-msi" },
    ]);
    expect(manifest.platforms["windows-x86_64-msi"]).toEqual({
      signature: "sig-msi",
      url: `https://github.com/${REPO}/releases/latest/download/Ken_0.2.0_x64_en-US.msi`,
    });
  });

  it("trims surrounding whitespace from signature text", () => {
    const assets = fullAssets();
    assets[0].sigText = "\n  dW50cnVzdGVkIGNvbW1lbnQ=  \n\n";
    const manifest = build(assets);
    expect(manifest.platforms["darwin-aarch64"].signature).toBe("dW50cnVzdGVkIGNvbW1lbnQ=");
  });

  it("ignores .sig assets it does not recognize", () => {
    const manifest = build([
      ...fullAssets(),
      { name: "ken-mcp-x86_64-unknown-linux-gnu.sig", sigText: "nope" },
      { name: "Ken-0.2.0.something-else.sig", sigText: "nope" },
    ]);
    expect(Object.keys(manifest.platforms)).toHaveLength(10);
  });

  it.each([
    ["darwin-aarch64", "Ken_aarch64.app.tar.gz.sig"],
    ["darwin-x86_64", "Ken_x64.app.tar.gz.sig"],
    ["linux-x86_64", "Ken_0.2.0_amd64.AppImage.sig"],
    ["windows-x86_64", "Ken_0.2.0_x64-setup.exe.sig"],
  ])("throws naming %s when its signed asset is missing", (platform, assetName) => {
    const assets = fullAssets().filter((a) => a.name !== assetName);
    expect(() => build(assets)).toThrowError(new RegExp(platform.replace(/\+/g, "\\+")));
  });

  it("keeps platform key order stable regardless of asset order", () => {
    const forward = Object.keys(build(fullAssets()).platforms);
    const reversed = Object.keys(build(fullAssets().reverse()).platforms);
    expect(reversed).toEqual(forward);
    expect(forward).toEqual([...forward].sort());
  });
});
