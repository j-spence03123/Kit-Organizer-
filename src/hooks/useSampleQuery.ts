import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorMessage } from "../services/api";
import type { SamplePage, SampleQuery } from "../types";

/** Loads one page of samples for a query; refetches when it changes. */
export function useSampleQuery(query: SampleQuery | null, version: number) {
  const [page, setPage] = useState<SamplePage>({ rows: [], total: 0 });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);
  const key = JSON.stringify(query);

  const load = useCallback(async () => {
    if (!query) return;
    const my = ++seq.current;
    setLoading(true);
    try {
      const res = await api.querySamples(query);
      if (my === seq.current) {
        setPage(res);
        setError(null);
      }
    } catch (e) {
      if (my === seq.current) setError(errorMessage(e));
    } finally {
      if (my === seq.current) setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  useEffect(() => {
    void load();
  }, [load, version]);

  return { page, loading, error, reload: load };
}

export function useDebounced<T>(value: T, ms = 250): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}
