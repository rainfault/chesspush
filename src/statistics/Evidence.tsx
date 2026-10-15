import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { getEvidence, type EvidenceSelector, type Report } from "./api";
import { errorMessage } from "../api";
const EvidenceContext = createContext<Report | null>(null);
export function EvidenceProvider({
  report,
  children,
}: {
  report: Report | null;
  children: ReactNode;
}) {
  return (
    <EvidenceContext.Provider value={report}>
      {children}
    </EvidenceContext.Provider>
  );
}
export function useEvidence<T>(preview: T[], selector?: EvidenceSelector) {
  const report = useContext(EvidenceContext),
    identity =
      (report?.evidence_revision ?? "") +
      "|" +
      selector?.kind +
      "|" +
      selector?.key;
  const [state, setState] = useState<{
    identity: string;
    items: T[];
    loaded: boolean;
  }>({ identity, items: preview, loaded: false });
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const current = useRef(identity);
  current.current = identity;
  const active =
    state.identity === identity
      ? state
      : { identity, items: preview, loaded: false };
  const total =
    selector && report
      ? (report.evidence_counts[selector.kind + ":" + selector.key] ??
        preview.length)
      : preview.length;
  async function more() {
    if (busy) return;
    if (!selector || !report) return;
    setBusy(true);
    setError("");
    const token = identity;
    try {
      const page = await getEvidence<T>(
        report,
        selector,
        active.loaded ? active.items.length : 0,
      );
      if (current.current === token)
        setState({
          identity: token,
          items: active.loaded ? [...active.items, ...page.items] : page.items,
          loaded: true,
        });
    } catch (e) {
      if (current.current === token) setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => setError(""), [identity]);
  useEffect(() => {
    if (!active.loaded && active.items.length === 0 && total > 0) void more();
  }, [identity, total]);
  return {
    items: active.items,
    total,
    busy,
    error,
    more,
    hasMore: active.items.length < total,
    loaded: active.loaded,
  };
}
export function MoreEvidence({
  value,
}: {
  value: {
    hasMore: boolean;
    busy: boolean;
    error: string;
    more: () => Promise<void>;
  };
}) {
  return (
    <>
      {value.error && <div role="alert">{value.error}</div>}
      {value.hasMore && (
        <button disabled={value.busy} onClick={() => void value.more()}>
          {value.busy ? "Загрузка" : "Ещё случаи"}
        </button>
      )}
    </>
  );
}
export function useEvidenceCounts() {
  return useContext(EvidenceContext)?.evidence_counts ?? {};
}
