import { useEffect, useRef, useState } from "react";
import { api, errorText, type Logs as LogChunk, type Service } from "../api";

const MAX_OUTPUT_CHARS = 160_000;
const POLL_MS = 700;

/** Service log viewer. Mount with a project-key so project changes discard its cursor. */
export function Logs({
  projectId,
  services,
  enabled,
}: {
  projectId: string;
  services: Service[];
  enabled: boolean;
}) {
  const [serviceId, setServiceId] = useState(services[0]?.id ?? "");
  const [text, setText] = useState("");
  const [paused, setPaused] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const cursor = useRef<{ offset: number; generation: string | null }>({ offset: 0, generation: null });
  const inFlight = useRef(false);
  const pollVersion = useRef(0);
  const outputRef = useRef<HTMLPreElement>(null);
  const selection = services.some((service) => service.id === serviceId) ? serviceId : services[0]?.id ?? "";
  const selected = services.find((service) => service.id === selection);

  useEffect(() => {
    cursor.current = { offset: 0, generation: null };
    setText("");
    setError("");
  }, [projectId, selection]);

  useEffect(() => {
    if (enabled && !paused && outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [enabled, paused, text]);

  useEffect(() => {
    if (!enabled || paused || !selection) return;
    let cancelled = false;
    let timer: number | undefined;
    let idleAfterExit = 0;
    const version = ++pollVersion.current;
    const current = () => !cancelled && version === pollVersion.current;

    const poll = async () => {
      if (!current()) return;
      // An invoke from the previous service/state effect may still be in
      // flight. Let it settle before issuing the newest cursor request.
      if (inFlight.current) {
        timer = window.setTimeout(() => void poll(), 100);
        return;
      }
      inFlight.current = true;
      setLoading(true);
      try {
        const before = cursor.current;
        const chunk: LogChunk & { generation: string; reset: boolean } = await api.followLogs(
          projectId,
          selection,
          before.offset,
          before.generation,
        );
        if (!current()) return;
        cursor.current = { offset: chunk.nextOffset, generation: chunk.generation };
        setText((previous) => {
          const combined = chunk.reset ? chunk.text : previous + chunk.text;
          return combined.length > MAX_OUTPUT_CHARS ? combined.slice(-MAX_OUTPUT_CHARS) : combined;
        });
        setError("");
        if (selected && !["starting", "running", "orphaned"].includes(selected.state)) {
          idleAfterExit = chunk.text.length === 0 ? idleAfterExit + 1 : 0;
        } else {
          idleAfterExit = 0;
        }
        // A few quiet reads after exit catch bytes flushed just after process
        // state changed; then keep the last output without polling indefinitely.
        if (idleAfterExit < 4) timer = window.setTimeout(() => void poll(), POLL_MS);
      } catch (err) {
        if (current()) {
          setError(errorText(err));
          timer = window.setTimeout(() => void poll(), 1_500);
        }
      } finally {
        inFlight.current = false;
        if (current()) setLoading(false);
      }
    };
    void poll();
    return () => {
      cancelled = true;
      if (pollVersion.current === version) pollVersion.current += 1;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [enabled, paused, projectId, selection, selected?.state]);

  return (
    <section className="block" aria-label="Service logs">
      <div className="row">
        <div className="field">
          <label htmlFor="log-service">Service</label>
          <select id="log-service" className="input" value={selection} onChange={(event) => setServiceId(event.target.value)} disabled={services.length === 0}>
            {services.length === 0 ? <option value="">No services</option> : null}
            {services.map((service) => <option key={service.id} value={service.id}>{service.id}</option>)}
          </select>
        </div>
        <button type="button" className="btn" disabled={!selection} onClick={() => setPaused((value) => !value)}>
          {paused ? "Resume following" : "Pause following"}
        </button>
      </div>
      <p className="hint">{selected ? `${selected.runtime || "service"} · ${selected.state}` : "Select a service to read its output."}{loading ? " · Updating…" : ""}</p>
      {error ? <p className="error" role="alert">Log read failed: {error}. Retrying…</p> : null}
      {text ? <pre ref={outputRef} className="output" tabIndex={0} aria-label={`${selection} service log`}>{text}</pre> : <p className="empty">No log output yet for {selection || "this project"}.</p>}
    </section>
  );
}
