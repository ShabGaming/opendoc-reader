import { act, render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "../App";
import { createApi } from "../api/api";
import { MockBackend, type MockOptions } from "../api/mock/backend";
import { createMockPlatform, type MockPlatformOptions } from "../api/mock/platform";
import type { DevParams } from "../api/runtime";
import { AppProvider } from "../state/AppContext";

interface RenderOptions extends MockOptions {
  platform?: MockPlatformOptions;
  dev?: Partial<DevParams>;
}

const noDev: DevParams = {
  mock: true,
  screen: null,
  theme: null,
  step: null,
  section: null,
  page: null,
  view: null,
  advanceMs: 0,
  freeze: true,
  hardware: null,
  fail: null,
  file: null,
  missing: null,
};

/**
 * Renders the whole app against a mock backend whose clock the test drives. Resolves once the
 * app has booted, so the first screen is on the page.
 */
export async function renderApp(options: RenderOptions = {}) {
  const backend = new MockBackend({ scenario: "demo", ...options });
  if (options.dev?.missing) backend.markOutputsGone(options.dev.missing);
  const platform = createMockPlatform({ openWindow: false, ...options.platform });
  const api = createApi(backend);
  const dev = options.dev ? { ...noDev, ...options.dev } : null;
  const user = userEvent.setup();
  const utils = render(
    <AppProvider api={api} platform={platform} dev={dev}>
      <App />
    </AppProvider>,
  );
  await flush();
  return {
    ...utils,
    backend,
    platform,
    api,
    user,
    /** Moves the mock clock and lets React settle. */
    advance: async (ms: number) => {
      await act(async () => {
        backend.advance(ms);
      });
    },
  };
}

/** Lets pending promises and their state updates finish. */
export async function flush(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}
