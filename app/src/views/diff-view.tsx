import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer, type VirtualItem } from "@tanstack/react-virtual";
import {
  AlertTriangle,
  Download,
  FileJson,
  GitCompare,
  Loader2,
  Save,
  Search,
  X,
} from "lucide-react";
import {
  browseDiffSavePath,
  browseDiffSource,
  browseInventorySavePath,
  diffCancelInventory,
  diffGenerateInventory,
  diffLoadInventoryReport,
  diffQueryReportPage,
  diffSaveDiffReport,
  diffSaveInventoryReport,
  onDiffInventoryProgress,
  type DiffItem,
  type DiffPage,
  type DiffStatus,
  type DiffTier,
} from "../lib/commands";
import {
  buildBackendFilter,
  emptySlot,
  useDiffStore,
  type SlotId,
  type SourceSlot,
  type StatusFilter,
  type TierFilter,
} from "../stores/diff-store";

function isInventoryPath(path: string): boolean {
  return path.toLowerCase().endsWith(".starbreaker-inventory.json");
}

function formatCount(value: number): string {
  return value.toLocaleString();
}

function statusLabel(status: DiffStatus): string {
  switch (status) {
    case "metadata_changed": return "metadata";
    default: return status;
  }
}

function tierLabel(tier: DiffTier): string {
  return tier === "data_core" ? "DataCore" : "P4k";
}

function itemPath(item: DiffItem): string {
  const side = item.new ?? item.old;
  if (!side) return item.display;
  if ("archive" in side) return side.archive.path;
  return side.data_core.path;
}

function itemType(item: DiffItem): string {
  const side = item.new ?? item.old;
  if (!side) return "";
  if ("archive" in side) {
    const match = side.archive.path.match(/\.([^.\\/]+)$/);
    return match ? `.${match[1].toLowerCase()}` : "";
  }
  return side.data_core.record_type;
}

export function DiffView() {
  const oldSlot = useDiffStore((s) => s.oldSlot);
  const newSlot = useDiffStore((s) => s.newSlot);
  const diff = useDiffStore((s) => s.diff);
  const compareIds = useDiffStore((s) => s.compareIds);
  const selectedKey = useDiffStore((s) => s.selectedKey);
  const error = useDiffStore((s) => s.error);
  const includeUnchanged = useDiffStore((s) => s.includeUnchanged);
  const search = useDiffStore((s) => s.search);
  const tier = useDiffStore((s) => s.tier);
  const status = useDiffStore((s) => s.status);
  const extension = useDiffStore((s) => s.extension);
  const recordType = useDiffStore((s) => s.recordType);
  const pathPrefix = useDiffStore((s) => s.pathPrefix);
  const storedCurrentOffset = useDiffStore((s) => s.currentOffset);
  const storedQueryKey = useDiffStore((s) => s.queryKey);
  const setSlot = useDiffStore((s) => s.setSlot);
  const updateSlot = useDiffStore((s) => s.updateSlot);
  const setDiff = useDiffStore((s) => s.setDiff);
  const setCompareIds = useDiffStore((s) => s.setCompareIds);
  const setSelectedKey = useDiffStore((s) => s.setSelectedKey);
  const setError = useDiffStore((s) => s.setError);
  const setIncludeUnchanged = useDiffStore((s) => s.setIncludeUnchanged);
  const setSearch = useDiffStore((s) => s.setSearch);
  const setTier = useDiffStore((s) => s.setTier);
  const setStatus = useDiffStore((s) => s.setStatus);
  const setExtension = useDiffStore((s) => s.setExtension);
  const setRecordType = useDiffStore((s) => s.setRecordType);
  const setPathPrefix = useDiffStore((s) => s.setPathPrefix);
  const setScrollTop = useDiffStore((s) => s.setScrollTop);
  const setCurrentOffset = useDiffStore((s) => s.setCurrentOffset);
  const setQueryKey = useDiffStore((s) => s.setQueryKey);
  const resetComparison = useDiffStore((s) => s.resetComparison);
  const [pageCache, setPageCache] = useState<Map<number, DiffPage>>(() => new Map());
  const [pageLoading, setPageLoading] = useState(false);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const cacheRef = useRef<Map<number, DiffPage>>(new Map());
  const inFlightRef = useRef<Map<number, Promise<DiffPage>>>(new Map());
  const queryKeyRef = useRef(storedQueryKey);
  const currentPageOffsetRef = useRef(storedCurrentOffset);
  const restoredSessionRef = useRef<string | null>(null);
  const scrollDebounceRef = useRef<number | null>(null);
  const lastScrollTopRef = useRef(0);
  const scrollDirectionRef = useRef<"up" | "down">("down");
  const rowHeight = 34;
  const pageSize = 1000;
  const pageCacheLimit = 7;
  const rowVirtualizer = useVirtualizer({
    count: diff?.total_matching ?? 0,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 30,
  });
  const virtualRows = rowVirtualizer.getVirtualItems();

  const loadSource = async (slot: SlotId) => {
    const path = await browseDiffSource();
    if (!path) return;
    setError(null);
    resetComparison();
    clearPageCache();

    if (isInventoryPath(path)) {
      setSlot(slot, { ...emptySlot, path, loading: true });
      try {
        const report = await diffLoadInventoryReport(path);
        setSlot(slot, { ...emptySlot, path, report });
      } catch (err) {
        setSlot(slot, { ...emptySlot, path, error: String(err) });
      }
      return;
    }

    setSlot(slot, { ...emptySlot, path, loading: true });
    const jobId = `${slot}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    const unlisten = await onDiffInventoryProgress((progress) => {
      if (progress.job_id !== jobId) return;
      updateSlot(slot, (current) => ({ ...current, loading: true, progress }));
    });
    try {
      const report = await diffGenerateInventory(path, false, jobId);
      setSlot(slot, { ...emptySlot, path, report });
    } catch (err) {
      setSlot(slot, { ...emptySlot, path, error: String(err) });
    } finally {
      unlisten();
    }
  };

  const compare = async () => {
    if (!oldSlot.report || !newSlot.report) return;
    setError(null);
    setDiff(null);
    clearPageCache();
    setCompareIds({ oldId: oldSlot.report.id, newId: newSlot.report.id });
    setCurrentOffset(0);
    setScrollTop(0);
    setQueryKey("");
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
  };

  function setSelectedFromPage(page: DiffPage) {
    const current = useDiffStore.getState().selectedKey;
    setSelectedKey(current && page.items.some((item) => item.key === current)
      ? current
      : page.items[0]?.key ?? null);
  }

  const backendFilter = useMemo(() => buildBackendFilter({
    search,
    tier,
    status,
    extension,
    recordType,
    pathPrefix,
    includeUnchanged,
  }), [extension, includeUnchanged, pathPrefix, recordType, search, status, tier]);
  const filterKey = useMemo(() => JSON.stringify(backendFilter), [backendFilter]);
  const queryKey = useMemo(() => {
    if (!compareIds) return "";
    return `${compareIds.oldId}:${compareIds.newId}:${filterKey}`;
  }, [compareIds, filterKey]);

  const normalizePageOffset = useCallback((offset: number) => {
    return Math.max(0, Math.floor(offset / pageSize) * pageSize);
  }, [pageSize]);

  const clearPageCache = useCallback(() => {
    cacheRef.current = new Map();
    inFlightRef.current = new Map();
    setPageCache(new Map());
  }, []);

  const storePage = useCallback((page: DiffPage) => {
    const nextCache = new Map(cacheRef.current).set(page.offset, page);
    while (nextCache.size > pageCacheLimit) {
      const farthestOffset = Array.from(nextCache.keys()).reduce((farthest, offset) => {
        return Math.abs(offset - currentPageOffsetRef.current) > Math.abs(farthest - currentPageOffsetRef.current)
          ? offset
          : farthest;
      });
      nextCache.delete(farthestOffset);
    }
    cacheRef.current = nextCache;
    setPageCache(cacheRef.current);
  }, [pageCacheLimit]);

  const fetchPage = useCallback(async (offset: number) => {
    if (!compareIds) return null;
    const pageOffset = normalizePageOffset(offset);
    const requestKey = queryKey;
    const cached = cacheRef.current.get(pageOffset);
    if (cached) return cached;
    const existing = inFlightRef.current.get(pageOffset);
    if (existing) return existing;
    let request: Promise<DiffPage>;
    request = diffQueryReportPage(
      compareIds.oldId,
      compareIds.newId,
      backendFilter,
      pageOffset,
      pageSize,
    ).then((page) => {
      if (queryKeyRef.current === requestKey) {
        storePage(page);
      }
      return page;
    }).finally(() => {
      if (inFlightRef.current.get(pageOffset) === request) {
        inFlightRef.current.delete(pageOffset);
      }
    });
    inFlightRef.current.set(pageOffset, request);
    return request;
  }, [backendFilter, compareIds, normalizePageOffset, pageSize, queryKey, storePage]);

  const prefetchPage = useCallback((offset: number) => {
    if (!compareIds || !diff) return;
    const pageOffset = normalizePageOffset(offset);
    if (pageOffset < 0 || pageOffset >= diff.total_matching) return;
    if (cacheRef.current.has(pageOffset) || inFlightRef.current.has(pageOffset)) return;
    void fetchPage(pageOffset).catch((err) => {
      console.warn("diff page prefetch failed", err);
    });
  }, [compareIds, diff, fetchPage, normalizePageOffset]);

  const loadPage = useCallback(async (offset: number) => {
    if (!compareIds) return;
    const pageOffset = normalizePageOffset(offset);
    const cached = cacheRef.current.get(pageOffset);
    if (cached) {
      setDiff(cached);
      setCurrentOffset(cached.offset);
      setSelectedFromPage(cached);
      return;
    }
    setError(null);
    setPageLoading(true);
    try {
      console.info("diff page request", {
        oldId: compareIds.oldId,
        newId: compareIds.newId,
        filter: backendFilter,
        offset: pageOffset,
        limit: pageSize,
      });
      const next = await fetchPage(pageOffset);
      if (!next || queryKeyRef.current !== queryKey) return;
      console.info("diff page response", {
        summary: next.summary,
        totalMatching: next.total_matching,
        offset: next.offset,
        returnedItems: next.items.length,
      });
      setDiff(next);
      setCurrentOffset(next.offset);
      setSelectedFromPage(next);
    } catch (err) {
      console.error("diff page failed", err);
      setError(String(err));
    } finally {
      setPageLoading(false);
    }
  }, [backendFilter, compareIds, fetchPage, normalizePageOffset, pageSize, queryKey, setCurrentOffset]);

  useEffect(() => {
    if (!compareIds) return;
    if (queryKeyRef.current === queryKey && storedQueryKey === queryKey) {
      if (diff && restoredSessionRef.current !== queryKey) {
        const nextCache = new Map([[diff.offset, diff]]);
        cacheRef.current = nextCache;
        setPageCache(nextCache);
        restoredSessionRef.current = queryKey;
        window.requestAnimationFrame(() => {
          if (scrollRef.current) scrollRef.current.scrollTop = useDiffStore.getState().scrollTop;
        });
      } else if (!diff) {
        void loadPage(storedCurrentOffset);
      }
      return;
    }

    queryKeyRef.current = queryKey;
    restoredSessionRef.current = queryKey;
    clearPageCache();
    setDiff(null);
    setSelectedKey(null);
    setCurrentOffset(0);
    setScrollTop(0);
    setQueryKey(queryKey);
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
    void loadPage(0);
  }, [
    clearPageCache,
    compareIds,
    diff,
    filterKey,
    loadPage,
    queryKey,
    setCurrentOffset,
    setDiff,
    setQueryKey,
    setScrollTop,
    setSelectedKey,
    storedCurrentOffset,
    storedQueryKey,
  ]);

  useEffect(() => {
    if (!compareIds || !diff || pageLoading) return;
    const firstRow = virtualRows[0]?.index ?? 0;
    const lastRow = virtualRows[virtualRows.length - 1]?.index ?? firstRow;
    const targetOffset = normalizePageOffset(firstRow);
    currentPageOffsetRef.current = targetOffset;
    setCurrentOffset(targetOffset);
    const visiblePageOffsets = pageOffsetsForRange(firstRow, lastRow, pageSize);
    const missingVisibleOffset = visiblePageOffsets.find((offset) => !cacheRef.current.has(offset));
    if (missingVisibleOffset != null) {
      if (scrollDebounceRef.current != null) window.clearTimeout(scrollDebounceRef.current);
      scrollDebounceRef.current = window.setTimeout(() => {
        void loadPage(missingVisibleOffset);
      }, 80);
    } else if (cacheRef.current.has(targetOffset)) {
      const cached = cacheRef.current.get(targetOffset);
      if (cached && cached.offset !== diff.offset) setDiff(cached);
    }
    return () => {
      if (scrollDebounceRef.current != null) window.clearTimeout(scrollDebounceRef.current);
    };
  }, [compareIds, diff, loadPage, normalizePageOffset, pageLoading, pageSize, setCurrentOffset, virtualRows]);

  useEffect(() => {
    if (!diff) return;
    const direction = scrollDirectionRef.current;
    const firstRow = virtualRows[0]?.index ?? diff.offset;
    const lastRow = virtualRows[virtualRows.length - 1]?.index ?? firstRow;
    const nextOffset = normalizePageOffset(lastRow) + pageSize;
    const previousOffset = normalizePageOffset(firstRow) - pageSize;
    if (direction === "down") {
      prefetchPage(nextOffset);
    } else {
      prefetchPage(previousOffset);
    }
  }, [diff, normalizePageOffset, pageSize, prefetchPage, virtualRows]);

  const saveInventory = async (slot: SourceSlot) => {
    if (!slot.report) return;
    const safeLabel = slot.report.label.replace(/[\\/:*?"<>|]/g, "_");
    const path = await browseInventorySavePath(`${safeLabel}.starbreaker-inventory.json`);
    if (path) await diffSaveInventoryReport(path, slot.report.id);
  };

  const saveDiff = async () => {
    if (!diff) return;
    const path = await browseDiffSavePath(`${diff.old_label}-to-${diff.new_label}.starbreaker-diff.json`);
    if (path) await diffSaveDiffReport(path, diff);
  };

  const selected = cachedItems(pageCache).find((item) => item.key === selectedKey) ?? diff?.items[0] ?? null;
  const totalChanged = diff
    ? diff.summary.added + diff.summary.removed + diff.summary.modified + diff.summary.metadata_changed
    : 0;
  const visibleRows = buildVisibleRows(pageCache, virtualRows, pageSize);

  return (
    <div className="flex flex-col h-full bg-bg overflow-hidden">
      <header className="h-[var(--toolbar-height)] border-b border-border px-4 flex items-center justify-between bg-bg-alt">
        <div className="flex items-center gap-2">
          <GitCompare size={18} className="text-primary" />
          <h1 className="font-semibold text-sm">Diff Viewer</h1>
        </div>
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-2 text-xs text-text-sub">
            <input
              type="checkbox"
              checked={includeUnchanged}
              onChange={(event) => setIncludeUnchanged(event.target.checked)}
            />
            Include unchanged
          </label>
          <button
            onClick={compare}
            disabled={!oldSlot.report || !newSlot.report}
            className="px-3 py-1.5 rounded-md bg-primary/15 text-text text-xs disabled:opacity-40"
          >
            Compare
          </button>
          <button
            onClick={saveDiff}
            disabled={!diff}
            title="Export diff"
            className="p-1.5 rounded-md hover:bg-surface disabled:opacity-40"
          >
            <Download size={15} />
          </button>
        </div>
      </header>

      <section className="grid grid-cols-2 gap-3 p-3 border-b border-border">
        <SourcePanel slotId="old" slot={oldSlot} onLoad={() => loadSource("old")} onSave={() => saveInventory(oldSlot)} />
        <SourcePanel slotId="new" slot={newSlot} onLoad={() => loadSource("new")} onSave={() => saveInventory(newSlot)} />
      </section>

      {error && (
        <div className="mx-3 mt-3 p-2 border border-danger/30 bg-danger/10 text-danger text-xs rounded-md">
          {error}
        </div>
      )}

      {diff && (
        <section className="px-3 py-2 border-b border-border">
          <div className="grid grid-cols-5 gap-2">
            <SummaryCell label="Added" value={diff.summary.added} tone="text-success" />
            <SummaryCell label="Removed" value={diff.summary.removed} tone="text-danger" />
            <SummaryCell label="Modified" value={diff.summary.modified} tone="text-warning" />
            <SummaryCell label="Metadata" value={diff.summary.metadata_changed} tone="text-info" />
            <SummaryCell label="Unchanged" value={diff.summary.unchanged} tone="text-text-dim" />
          </div>
          <p className="mt-2 text-xs text-text-dim">
            {diff.total_matching === 0
              ? "No matching rows."
              : `${formatCount(diff.total_matching)} matching rows.`}
            {diff.total_matching !== totalChanged && ` ${formatCount(totalChanged)} changed rows before filters.`}
          </p>
        </section>
      )}

      <div className="flex flex-1 min-h-0">
        <aside className="w-[240px] border-r border-border p-3 flex flex-col gap-3 bg-bg-alt/40">
          <div className="relative">
            <Search size={14} className="absolute left-2 top-2.5 text-text-dim" />
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search"
              className="w-full pl-7 pr-2 py-2 bg-surface border border-border rounded-md text-sm outline-none focus:border-primary"
            />
          </div>
          <FilterSelect label="Tier" value={tier} onChange={(value) => setTier(value as TierFilter)} options={[
            ["all", "All"],
            ["p4k", "P4k files"],
            ["data_core", "DataCore"],
          ]} />
          <FilterSelect label="Status" value={status} onChange={(value) => setStatus(value as StatusFilter)} options={[
            ["all", "All"],
            ["added", "Added"],
            ["removed", "Removed"],
            ["modified", "Modified"],
            ["metadata_changed", "Metadata"],
            ["unchanged", "Unchanged"],
          ]} />
          <TextFilter label="Extension" value={extension} onChange={setExtension} placeholder=".dds" />
          <TextFilter label="Record type" value={recordType} onChange={setRecordType} placeholder="EntityClassDefinition" />
          <TextFilter label="Path prefix" value={pathPrefix} onChange={setPathPrefix} placeholder="Data/Objects" />
        </aside>

        <main className="flex-1 min-w-0 flex flex-col">
          <div className="grid grid-cols-[80px_76px_minmax(140px,1fr)_130px_minmax(120px,0.8fr)] gap-3 px-3 py-2 text-[11px] uppercase tracking-wide text-text-dim border-b border-border">
            <span>Status</span>
            <span>Tier</span>
            <span>Name</span>
            <span>Type</span>
            <span>Reasons</span>
          </div>
          <div
            ref={scrollRef}
            className="relative flex-1 overflow-auto"
            onScroll={(event) => {
              const nextScrollTop = event.currentTarget.scrollTop;
              scrollDirectionRef.current = nextScrollTop >= lastScrollTopRef.current ? "down" : "up";
              lastScrollTopRef.current = nextScrollTop;
              setScrollTop(nextScrollTop);
            }}
          >
            <div style={{ height: rowVirtualizer.getTotalSize(), position: "relative" }}>
              {visibleRows.map(({ item, index, start }) => (
                <div
                  key={`${item?.tier ?? "loading"}:${item?.key ?? index}`}
                  style={{ position: "absolute", top: start, left: 0, right: 0, height: rowHeight }}
                >
                  {item ? (
                    <button
                      onClick={() => setSelectedKey(item.key)}
                      className={`grid grid-cols-[80px_76px_minmax(140px,1fr)_130px_minmax(120px,0.8fr)] gap-3 w-full px-3 text-left text-xs border-b border-border/60 hover:bg-surface/60 ${
                        selected?.key === item.key ? "bg-primary/10" : ""
                      }`}
                      style={{ height: rowHeight }}
                    >
                      <span className={`self-center ${statusTone(item.status)}`}>{statusLabel(item.status)}</span>
                      <span className="self-center text-text-sub">{tierLabel(item.tier)}</span>
                      <span className="self-center truncate" title={item.display}>{item.display}</span>
                      <span className="self-center truncate text-text-sub" title={itemType(item)}>{itemType(item)}</span>
                      <span className="self-center truncate text-text-dim" title={item.reasons.join(", ")}>{item.reasons.join(", ")}</span>
                    </button>
                  ) : (
                    <div className="grid grid-cols-[80px_76px_minmax(140px,1fr)_130px_minmax(120px,0.8fr)] gap-3 w-full px-3 text-left text-xs border-b border-border/60 text-text-dim" style={{ height: rowHeight }}>
                      <span className="self-center">...</span>
                      <span className="self-center">...</span>
                      <span className="self-center">Loading row {formatCount(index + 1)}</span>
                    </div>
                  )}
                </div>
              ))}
            </div>
            {pageLoading && (
              <div className="absolute right-4 bottom-4 flex items-center gap-2 rounded-md border border-border bg-bg-alt px-3 py-2 text-xs text-text-sub shadow">
                <Loader2 size={14} className="animate-spin" />
                Loading rows
              </div>
            )}
          </div>
        </main>

        <aside className="w-[320px] border-l border-border p-3 bg-bg-alt/40 overflow-auto">
          <DetailPanel item={selected} />
        </aside>
      </div>
    </div>
  );
}

function cachedItems(cache: Map<number, DiffPage>): DiffItem[] {
  return Array.from(cache.values()).flatMap((page) => page.items);
}

function buildVisibleRows(
  cache: Map<number, DiffPage>,
  virtualRows: VirtualItem[],
  pageSize: number,
): Array<{ item: DiffItem | null; index: number; start: number }> {
  return virtualRows.map((row) => {
    const index = row.index;
    const pageOffset = Math.floor(index / pageSize) * pageSize;
    const page = cache.get(pageOffset);
    const item = page?.items[index - pageOffset];
    return { item: item ?? null, index, start: row.start };
  });
}

function pageOffsetsForRange(start: number, end: number, pageSize: number): number[] {
  const offsets = [];
  const firstOffset = Math.floor(start / pageSize) * pageSize;
  const lastOffset = Math.floor(end / pageSize) * pageSize;
  for (let offset = firstOffset; offset <= lastOffset; offset += pageSize) {
    offsets.push(offset);
  }
  return offsets;
}

function SourcePanel({ slotId, slot, onLoad, onSave }: {
  slotId: SlotId;
  slot: SourceSlot;
  onLoad: () => void;
  onSave: () => void;
}) {
  return (
    <div className="border border-border rounded-md bg-bg-alt p-3 min-w-0">
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="text-[11px] uppercase tracking-wide text-text-dim">{slotId === "old" ? "Old" : "New"}</p>
          <p className="text-sm font-medium truncate">{slot.report?.label ?? "No source selected"}</p>
        </div>
        <div className="flex items-center gap-1">
          {slot.loading && (
            <button onClick={() => diffCancelInventory()} title="Cancel" className="p-1.5 rounded-md hover:bg-surface">
              <X size={14} />
            </button>
          )}
          <button onClick={onSave} disabled={!slot.report} title="Save inventory" className="p-1.5 rounded-md hover:bg-surface disabled:opacity-40">
            <Save size={14} />
          </button>
          <button onClick={onLoad} title="Load source" className="p-1.5 rounded-md hover:bg-surface">
            <FileJson size={14} />
          </button>
        </div>
      </div>
      {slot.path && <p className="mt-2 text-xs text-text-dim truncate" title={slot.path}>{slot.path}</p>}
      {slot.loading && (
        <div className="mt-3 grid grid-cols-[16px_minmax(0,1fr)_96px] items-center gap-2 text-xs text-text-sub">
          <Loader2 size={14} className="animate-spin" />
          <span className="truncate">{slot.progress?.message ?? "Generating inventory"}</span>
          {slot.progress?.total != null && (
            <span className="text-text-dim text-right tabular-nums">
              {formatCount(slot.progress.current ?? 0)}/{formatCount(slot.progress.total)}
            </span>
          )}
        </div>
      )}
      {slot.report && (
        <p className="mt-2 text-xs text-text-dim">
          {formatCount(slot.report.archive_count)} files · {formatCount(slot.report.datacore_count)} records
        </p>
      )}
      {slot.error && (
        <div className="mt-2 flex gap-2 text-xs text-danger">
          <AlertTriangle size={14} />
          <span>{slot.error}</span>
        </div>
      )}
    </div>
  );
}

function SummaryCell({ label, value, tone }: { label: string; value: number; tone: string }) {
  return (
    <div className="bg-bg-alt border border-border rounded-md px-3 py-2">
      <p className="text-[11px] uppercase tracking-wide text-text-dim">{label}</p>
      <p className={`text-lg font-semibold ${tone}`}>{formatCount(value)}</p>
    </div>
  );
}

function FilterSelect({ label, value, onChange, options }: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: Array<[string, string]>;
}) {
  return (
    <label className="flex flex-col gap-1 text-xs text-text-dim">
      {label}
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="bg-surface border border-border rounded-md px-2 py-2 text-sm text-text outline-none focus:border-primary"
      >
        {options.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
      </select>
    </label>
  );
}

function TextFilter({ label, value, onChange, placeholder }: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
}) {
  return (
    <label className="flex flex-col gap-1 text-xs text-text-dim">
      {label}
      <input
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        className="bg-surface border border-border rounded-md px-2 py-2 text-sm text-text outline-none focus:border-primary"
      />
    </label>
  );
}

function DetailPanel({ item }: { item: DiffItem | null }) {
  if (!item) {
    return <p className="text-sm text-text-dim">Select a changed item.</p>;
  }
  return (
    <div className="flex flex-col gap-3">
      <div>
        <p className="text-[11px] uppercase tracking-wide text-text-dim">{tierLabel(item.tier)}</p>
        <h2 className="text-sm font-semibold break-words">{item.display}</h2>
      </div>
      <DetailRow label="Status" value={statusLabel(item.status)} />
      <DetailRow label="Key" value={item.key} />
      <DetailRow label="Path" value={itemPath(item)} />
      <DetailRow label="Type" value={itemType(item)} />
      <div>
        <p className="text-[11px] uppercase tracking-wide text-text-dim mb-1">Reasons</p>
        {item.reasons.length === 0 ? (
          <p className="text-xs text-text-dim">No change reasons.</p>
        ) : (
          <div className="flex flex-wrap gap-1">
            {item.reasons.map((reason) => (
              <span key={reason} className="px-2 py-1 bg-surface border border-border rounded text-xs">
                {reason}
              </span>
            ))}
          </div>
        )}
      </div>
      <SideBlock title="Old" side={item.old} />
      <SideBlock title="New" side={item.new} />
    </div>
  );
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-[11px] uppercase tracking-wide text-text-dim">{label}</p>
      <p className="text-xs break-words">{value || "n/a"}</p>
    </div>
  );
}

function SideBlock({ title, side }: { title: string; side: DiffItem["old"] }) {
  if (!side) return <DetailRow label={title} value="missing" />;
  if ("archive" in side) {
    const entry = side.archive;
    return (
      <div className="border border-border rounded-md p-2">
        <p className="text-[11px] uppercase tracking-wide text-text-dim mb-1">{title}</p>
        <p className="text-xs break-words">{entry.path}</p>
        <p className="text-xs text-text-dim mt-1">CRC32 {entry.crc32} · {formatCount(entry.uncompressed_size)} bytes</p>
      </div>
    );
  }
  const record = side.data_core;
  return (
    <div className="border border-border rounded-md p-2">
      <p className="text-[11px] uppercase tracking-wide text-text-dim mb-1">{title}</p>
      <p className="text-xs break-words">{record.name}</p>
      <p className="text-xs text-text-dim mt-1">{record.record_type}</p>
      <p className="text-xs text-text-dim break-words mt-1">{record.content_hash}</p>
    </div>
  );
}

function statusTone(status: DiffStatus): string {
  switch (status) {
    case "added": return "text-success";
    case "removed": return "text-danger";
    case "modified": return "text-warning";
    case "metadata_changed": return "text-info";
    case "unchanged": return "text-text-dim";
  }
}
