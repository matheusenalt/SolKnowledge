/** Coordinates debounced IPC queries; never cancels or replaces persisted data. */
export function liveSearch<T, Scope>(options: {
  search: (query: string, scope: Scope) => Promise<T>;
  pending: () => void;
  start: (query: string) => void;
  results: (value: T) => void;
  error: (error: unknown) => void;
  clear: () => void;
  delay?: number;
  schedule?: (run: () => void, delay: number) => ReturnType<typeof setTimeout>;
  unschedule?: (timer: ReturnType<typeof setTimeout>) => void;
}) {
  let revision = 0;
  let key: string | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let queued: (() => void) | undefined;
  const schedule = options.schedule ?? setTimeout;
  const unschedule = options.unschedule ?? clearTimeout;
  function cancel() {
    revision++;
    key = undefined;
    if (timer !== undefined) unschedule(timer);
    timer = undefined;
    queued = undefined;
  }
  function input(value: string, scope: Scope, immediate = false) {
    const query = value.trim();
    const next = JSON.stringify([query, scope]);
    if (next === key) {
      if (immediate && queued) {
        if (timer !== undefined) unschedule(timer);
        queued();
      }
      return;
    }
    cancel();
    key = next;
    const current = revision;
    options.pending();
    if (!query) {
      options.clear();
      return;
    }
    const run = () => {
      timer = undefined;
      queued = undefined;
      options.start(query);
      void Promise.resolve()
        .then(() => options.search(query, scope))
        .then((value) => {
          if (current === revision) options.results(value);
        })
        .catch((error: unknown) => {
          if (current === revision) {
            key = undefined; // Explicit submit can retry a failed query.
            options.error(error);
          }
        });
    };
    queued = run;
    if (immediate) run();
    else timer = schedule(run, options.delay ?? 200);
  }
  return { input, cancel };
}
