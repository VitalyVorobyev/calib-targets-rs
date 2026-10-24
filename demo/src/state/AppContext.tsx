// The app's single store: one reducer, plus the actions that talk to the worker.

import {
  createContext,
  use,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  type Dispatch,
  type ReactNode,
} from "react";

import { renderRequest, KIND_LABEL, stemOf } from "../generate/request";
import { pngBlob } from "../lib/download";
import { loadImageBlob, loadImageUrl, revokeImage, type ImageRecord } from "../lib/image";
import { sampleBoard, type Sample } from "../lib/samples";
import { boardOfTarget, type BoardSync } from "../lib/boards";
import { worker, CancelledError } from "../worker/client";
import type { DetectMode } from "../worker/protocol";
import { inputOf } from "./input";
import { INITIAL_STATE, reducer } from "./reducer";
import type { Action, AppState } from "./types";

export interface Actions {
  detect: () => void;
  cancel: () => void;
  loadFile: (file: File) => void;
  loadSample: (sample: Sample) => void;
  generate: () => void;
}

const StateContext = createContext<AppState>(INITIAL_STATE);
const DispatchContext = createContext<Dispatch<Action>>(() => undefined);
const ActionsContext = createContext<Actions | null>(null);

export const useAppState = (): AppState => use(StateContext);
export const useDispatch = (): Dispatch<Action> => use(DispatchContext);
export function useActions(): Actions {
  const actions = use(ActionsContext);
  if (actions === null) throw new Error("useActions must be used inside <AppProvider>.");
  return actions;
}

const errorMessage = (error: unknown): string => (error instanceof Error ? error.message : String(error));

/** Auto-detect waits for the user to stop editing before it spends a run on the values. */
const AUTO_DELAY_MS = 400;

const MODE_OF_SYNC = (sync: BoardSync): DetectMode => sync.mode;

export function AppProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, INITIAL_STATE);
  const latest = useRef(state);
  useLayoutEffect(() => {
    latest.current = state;
  });
  const loadSeq = useRef(0);

  useEffect(() => {
    worker.defaults().then(
      (defaults) => dispatch({ type: "booted", defaults }),
      (error: unknown) => dispatch({ type: "bootFailed", message: errorMessage(error) }),
    );
  }, []);

  // The object URL of an image is released when the next one replaces it.
  const { image } = state;
  useEffect(() => () => revokeImage(image), [image]);

  const detect = useCallback(() => {
    const s = latest.current;
    const input = inputOf(s);
    if (s.image === null || input === null || s.run.status === "running") return;
    const rev = s.rev;
    dispatch({ type: "runStarted" });
    worker.detect(s.image, input).then(
      (outcome) => dispatch({ type: "runFinished", rev, outcome }),
      (error: unknown) => {
        if (!(error instanceof CancelledError)) dispatch({ type: "runFailed", rev, message: errorMessage(error) });
      },
    );
  }, []);

  const cancel = useCallback(() => {
    worker.cancel();
    dispatch({ type: "runCancelled" });
  }, []);

  // An edit while a run is in flight makes that run's answer worthless: stop it.
  const stale = state.run.status === "running" && state.run.rev !== state.rev;
  useEffect(() => {
    if (stale) cancel();
  }, [stale, cancel]);

  const hasResult = state.run.status !== "idle" && state.run.status !== "running" && state.run.rev === state.rev;
  const wantRun =
    state.auto && state.image !== null && state.chess !== null && state.run.status !== "running" && !hasResult;
  useEffect(() => {
    if (!wantRun) return;
    const timer = setTimeout(detect, AUTO_DELAY_MS);
    return () => clearTimeout(timer);
  }, [wantRun, state.rev, detect]);

  /** Load an image, ignoring the answer if a newer load started meanwhile. */
  const load = useCallback((name: string, make: () => Promise<ImageRecord>, then: (image: ImageRecord) => Action) => {
    const seq = ++loadSeq.current;
    dispatch({ type: "imageLoading", name });
    make().then(
      (loaded) => {
        if (seq === loadSeq.current) dispatch(then(loaded));
        else revokeImage(loaded);
      },
      (error: unknown) => {
        if (seq === loadSeq.current)
          dispatch({ type: "imageFailed", message: `Could not open ${name}: ${errorMessage(error)}` });
      },
    );
  }, []);

  const loadFile = useCallback(
    (file: File) =>
      load(
        file.name,
        () => loadImageBlob(file, file.name),
        (loaded) => ({ type: "imageLoaded", image: loaded }),
      ),
    [load],
  );

  const loadSample = useCallback(
    (sample: Sample) =>
      load(
        sample.label,
        () => loadImageUrl(sample.url, sample.label),
        (loaded): Action => ({
          type: "imageLoaded",
          image: loaded,
          sync: sampleBoard(sample, latest.current.params),
          mode: sample.mode,
        }),
      ),
    [load],
  );

  const generate = useCallback(() => {
    const form = latest.current.gen;
    dispatch({ type: "genStatus", status: { status: "busy" } });
    const seq = ++loadSeq.current;
    worker
      .render(renderRequest(form))
      .then(async (rendered) => {
        const stem = stemOf(form);
        const loaded = await loadImageBlob(pngBlob(rendered.bundle.png_bytes), `${stem}.png`);
        if (seq !== loadSeq.current) return revokeImage(loaded);
        const sync = boardOfTarget(rendered.target);
        dispatch({ type: "generated", generated: { label: `${KIND_LABEL[form.kind]}, ${stem}`, stem, rendered } });
        dispatch({ type: "imageLoaded", image: loaded, sync, mode: MODE_OF_SYNC(sync) });
      })
      .catch((error: unknown) =>
        dispatch({ type: "genStatus", status: { status: "error", message: errorMessage(error) } }),
      );
  }, []);

  const actions = useMemo<Actions>(
    () => ({ detect, cancel, loadFile, loadSample, generate }),
    [detect, cancel, loadFile, loadSample, generate],
  );

  return (
    <StateContext value={state}>
      <DispatchContext value={dispatch}>
        <ActionsContext value={actions}>{children}</ActionsContext>
      </DispatchContext>
    </StateContext>
  );
}
