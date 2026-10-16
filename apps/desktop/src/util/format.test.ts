import { describe, expect, it } from "vitest";
import {
  baseName,
  driveOf,
  fileName,
  formatBytes,
  formatDiskMB,
  formatDuration,
  formatMemoryMB,
  formatPageTime,
  formatProgressBytes,
  formatSpeed,
  parentDir,
} from "./format";

describe("formatBytes", () => {
  it("uses decimal units and no decimals for large megabyte values", () => {
    expect(formatBytes(1_988_649_560)).toBe("1.9 GB");
    expect(formatBytes(3_613_101_480)).toBe("3.6 GB");
    expect(formatBytes(790_000_000)).toBe("790 MB");
    expect(formatBytes(41_800_000)).toBe("41.8 MB");
    expect(formatBytes(412_000)).toBe("412 KB");
    expect(formatBytes(512)).toBe("512 B");
  });

  it("shows progress in the unit of the total", () => {
    expect(formatProgressBytes(910_000_000, 1_800_000_000)).toBe("0.91 / 1.80 GB");
    expect(formatProgressBytes(100_000_000, 410_000_000)).toBe("100 / 410 MB");
    expect(formatSpeed(12_300_000)).toBe("12.3 MB/s");
  });

  it("converts memory and disk sizes from MB", () => {
    expect(formatMemoryMB(65536)).toBe("64 GB");
    expect(formatDiskMB(396_000)).toBe("396 GB");
  });
});

describe("durations", () => {
  it("writes seconds, minutes and hours the way the screens do", () => {
    expect(formatDuration(14)).toBe("14 s");
    expect(formatDuration(130)).toBe("2 min 10 s");
    expect(formatDuration(120)).toBe("2 min");
    expect(formatDuration(3900)).toBe("1 h 5 min");
    expect(formatDuration(-3)).toBe("0 s");
  });

  it("gives one decimal for a page under ten seconds", () => {
    expect(formatPageTime(4.8)).toBe("4.8 s");
    expect(formatPageTime(42)).toBe("42 s");
  });
});

describe("paths", () => {
  it("splits Windows and macOS paths", () => {
    expect(fileName("C:\\Users\\Ada\\a b.pdf")).toBe("a b.pdf");
    expect(fileName("/Users/ada/a.pdf")).toBe("a.pdf");
    expect(parentDir("C:\\Users\\Ada\\a.pdf")).toBe("C:\\Users\\Ada");
    expect(parentDir("/Users/ada/a.pdf")).toBe("/Users/ada");
    expect(baseName("Library scans.pdf")).toBe("Library scans");
    expect(baseName("archive.tar.gz")).toBe("archive.tar");
    expect(baseName(".hidden")).toBe(".hidden");
  });

  it("names the drive of a Windows path only", () => {
    expect(driveOf("g:\\OpenDoc Reader\\models")).toBe("G:");
    expect(driveOf("/Users/ada")).toBeNull();
  });
});
