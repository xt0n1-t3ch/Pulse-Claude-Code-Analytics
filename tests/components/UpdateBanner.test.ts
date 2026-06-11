import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import type { AppUpdateInfo } from "@/lib/api";

const SKIP_KEY = "pulse-update-skipped-version";

function makeUpdate(overrides: Partial<AppUpdateInfo> = {}): AppUpdateInfo {
  return {
    current_version: "1.1.0",
    latest_version: "1.2.0",
    update_available: true,
    release_name: "Pulse 1.2.0",
    release_notes: "Faster reports and a new updater.",
    release_url: "https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.2.0",
    published_at: "2026-06-01T00:00:00Z",
    checked_at: "2026-06-10T00:00:00Z",
    assets: [],
    ...overrides,
  };
}

const checkAppUpdate = vi.fn<[], Promise<AppUpdateInfo>>();
const openAppReleasePage = vi.fn<[(string | null | undefined)?], Promise<void>>();

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    checkAppUpdate: () => checkAppUpdate(),
    openAppReleasePage: (url?: string | null) => openAppReleasePage(url),
  };
});

function setSearch(search: string): void {
  window.history.replaceState(null, "", search || window.location.pathname);
}

async function loadBanner() {
  return (await import("@/components/UpdateBanner.svelte")).default;
}

describe("UpdateBanner.svelte", () => {
  beforeEach(() => {
    checkAppUpdate.mockReset();
    openAppReleasePage.mockReset();
    openAppReleasePage.mockResolvedValue(undefined);
    localStorage.clear();
    setSearch("");
  });

  afterEach(() => {
    setSearch("");
  });

  it("renders the popup when an update is available", async () => {
    checkAppUpdate.mockResolvedValue(makeUpdate());
    const UpdateBanner = await loadBanner();
    const { findByText } = render(UpdateBanner);

    expect(await findByText("Update available")).toBeTruthy();
    expect(await findByText("1.1.0 → 1.2.0")).toBeTruthy();
    expect(checkAppUpdate).toHaveBeenCalledTimes(1);
  });

  it("stays hidden when there is no update", async () => {
    checkAppUpdate.mockResolvedValue(makeUpdate({ update_available: false, latest_version: null }));
    const UpdateBanner = await loadBanner();
    const { container } = render(UpdateBanner);

    await waitFor(() => expect(checkAppUpdate).toHaveBeenCalledTimes(1));
    expect(container.querySelector(".update-pop")).toBeNull();
  });

  it("Later dismisses the popup without persisting a skip", async () => {
    checkAppUpdate.mockResolvedValue(makeUpdate());
    const UpdateBanner = await loadBanner();
    const { container, findByText } = render(UpdateBanner);

    await findByText("Update available");
    await fireEvent.click(await findByText("Later"));

    await waitFor(
      () => expect(container.querySelector(".update-pop")).toBeNull(),
      { timeout: 2000 },
    );
    expect(localStorage.getItem(SKIP_KEY)).toBeNull();
  });

  it("Skip version persists the latest version to localStorage", async () => {
    checkAppUpdate.mockResolvedValue(makeUpdate());
    const UpdateBanner = await loadBanner();
    const { container, findByText } = render(UpdateBanner);

    await findByText("Update available");
    await fireEvent.click(await findByText("Skip version"));

    expect(localStorage.getItem(SKIP_KEY)).toBe("1.2.0");
    await waitFor(
      () => expect(container.querySelector(".update-pop")).toBeNull(),
      { timeout: 2000 },
    );
  });

  it("does not show a previously skipped version on auto-check", async () => {
    localStorage.setItem(SKIP_KEY, "1.2.0");
    checkAppUpdate.mockResolvedValue(makeUpdate());
    const UpdateBanner = await loadBanner();
    const { container } = render(UpdateBanner);

    await waitFor(() => expect(checkAppUpdate).toHaveBeenCalledTimes(1));
    expect(container.querySelector(".update-pop")).toBeNull();
  });

  it("forces a skipped version to show on the pulse:check-updates event", async () => {
    localStorage.setItem(SKIP_KEY, "1.2.0");
    checkAppUpdate.mockResolvedValue(makeUpdate());
    const UpdateBanner = await loadBanner();
    const { container, findByText } = render(UpdateBanner);

    await waitFor(() => expect(checkAppUpdate).toHaveBeenCalledTimes(1));
    expect(container.querySelector(".update-pop")).toBeNull();

    window.dispatchEvent(new CustomEvent("pulse:check-updates"));

    expect(await findByText("Update available")).toBeTruthy();
  });

  it("Open release calls openAppReleasePage with the release url", async () => {
    checkAppUpdate.mockResolvedValue(makeUpdate());
    const UpdateBanner = await loadBanner();
    const { findByText } = render(UpdateBanner);

    await findByText("Update available");
    await fireEvent.click(await findByText("Open release"));

    await waitFor(() => expect(openAppReleasePage).toHaveBeenCalledTimes(1));
    expect(openAppReleasePage).toHaveBeenCalledWith(
      "https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.2.0",
    );
  });

  it("synthesizes a fake update from ?fakeUpdate without calling the backend", async () => {
    setSearch("?fakeUpdate=9.9.9");
    const UpdateBanner = await loadBanner();
    const { findByText } = render(UpdateBanner);

    expect(await findByText("Update available")).toBeTruthy();
    expect(await findByText("dev → 9.9.9")).toBeTruthy();
    expect(checkAppUpdate).not.toHaveBeenCalled();

    await fireEvent.click(await findByText("Open release"));
    await waitFor(() => expect(openAppReleasePage).toHaveBeenCalledTimes(1));
    expect(openAppReleasePage).toHaveBeenCalledWith(
      "https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v9.9.9",
    );
  });
});
