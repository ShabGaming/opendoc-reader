import { useCallback, useEffect, useState } from "react";
import { useApp } from "../../state/AppContext";
import type { HardwareInfo, Recommendation } from "../../types/commands";

export type ProbeState =
  | { status: "loading" }
  | { status: "failed" }
  | { status: "ready"; hardware: HardwareInfo; recommendation: Recommendation };

/** Probes the computer, then asks which models fit. `retry` runs both again. */
export function useSetupProbe(): { probe: ProbeState; retry: () => void } {
  const { api, dispatch, state } = useApp();
  const [probe, setProbe] = useState<ProbeState>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const hasSelection = state.setupUi.selected !== null;

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const hardware = await api.probeHardware();
        const recommendation = await api.getRecommendation();
        if (cancelled) return;
        setProbe({ status: "ready", hardware, recommendation });
        if (!hasSelection) {
          const pick = recommendation.fits.find((f) => f.modelId === recommendation.modelId);
          dispatch({
            type: "setupSelect",
            selected: pick && pick.badge !== "wontFit" ? [recommendation.modelId] : [],
          });
        }
      } catch {
        if (!cancelled) setProbe({ status: "failed" });
      }
    })();
    return () => {
      cancelled = true;
    };
    // The recommended model is only applied the first time: a later probe must not undo the
    // user's choice, so `hasSelection` is read once per attempt.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, dispatch, attempt]);

  const retry = useCallback(() => {
    setProbe({ status: "loading" });
    setAttempt((n) => n + 1);
  }, []);

  return { probe, retry };
}
