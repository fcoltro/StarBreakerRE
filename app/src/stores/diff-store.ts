import { create } from "zustand";
import type {
  DiffFilter,
  DiffInventoryHandle,
  DiffInventoryProgress,
  DiffPage,
  DiffStatus,
  DiffTier,
} from "../lib/commands";

export type SlotId = "old" | "new";
export type StatusFilter = DiffStatus | "all";
export type TierFilter = DiffTier | "all";

export interface SourceSlot {
  path: string | null;
  report: DiffInventoryHandle | null;
  loading: boolean;
  error: string | null;
  progress: DiffInventoryProgress | null;
}

export const emptySlot: SourceSlot = {
  path: null,
  report: null,
  loading: false,
  error: null,
  progress: null,
};

interface DiffState {
  oldSlot: SourceSlot;
  newSlot: SourceSlot;
  diff: DiffPage | null;
  compareIds: { oldId: string; newId: string } | null;
  selectedKey: string | null;
  error: string | null;
  includeUnchanged: boolean;
  search: string;
  tier: TierFilter;
  status: StatusFilter;
  extension: string;
  recordType: string;
  pathPrefix: string;
  scrollTop: number;
  currentOffset: number;
  queryKey: string;

  setSlot: (slot: SlotId, next: SourceSlot) => void;
  updateSlot: (slot: SlotId, update: (current: SourceSlot) => SourceSlot) => void;
  setDiff: (diff: DiffPage | null) => void;
  setCompareIds: (compareIds: { oldId: string; newId: string } | null) => void;
  setSelectedKey: (selectedKey: string | null) => void;
  setError: (error: string | null) => void;
  setIncludeUnchanged: (includeUnchanged: boolean) => void;
  setSearch: (search: string) => void;
  setTier: (tier: TierFilter) => void;
  setStatus: (status: StatusFilter) => void;
  setExtension: (extension: string) => void;
  setRecordType: (recordType: string) => void;
  setPathPrefix: (pathPrefix: string) => void;
  setScrollTop: (scrollTop: number) => void;
  setCurrentOffset: (currentOffset: number) => void;
  setQueryKey: (queryKey: string) => void;
  resetComparison: () => void;
}

export const useDiffStore = create<DiffState>((set) => ({
  oldSlot: emptySlot,
  newSlot: emptySlot,
  diff: null,
  compareIds: null,
  selectedKey: null,
  error: null,
  includeUnchanged: false,
  search: "",
  tier: "all",
  status: "all",
  extension: "",
  recordType: "",
  pathPrefix: "",
  scrollTop: 0,
  currentOffset: 0,
  queryKey: "",

  setSlot: (slot, next) => set(slot === "old" ? { oldSlot: next } : { newSlot: next }),
  updateSlot: (slot, update) =>
    set((state) => (
      slot === "old"
        ? { oldSlot: update(state.oldSlot) }
        : { newSlot: update(state.newSlot) }
    )),
  setDiff: (diff) => set({ diff }),
  setCompareIds: (compareIds) => set({ compareIds }),
  setSelectedKey: (selectedKey) => set({ selectedKey }),
  setError: (error) => set({ error }),
  setIncludeUnchanged: (includeUnchanged) => set({ includeUnchanged }),
  setSearch: (search) => set({ search }),
  setTier: (tier) => set({ tier }),
  setStatus: (status) => set({ status }),
  setExtension: (extension) => set({ extension }),
  setRecordType: (recordType) => set({ recordType }),
  setPathPrefix: (pathPrefix) => set({ pathPrefix }),
  setScrollTop: (scrollTop) => set({ scrollTop }),
  setCurrentOffset: (currentOffset) => set({ currentOffset }),
  setQueryKey: (queryKey) => set({ queryKey }),
  resetComparison: () => set({
    diff: null,
    compareIds: null,
    selectedKey: null,
    scrollTop: 0,
    currentOffset: 0,
    queryKey: "",
  }),
}));

export function buildBackendFilter(input: {
  search: string;
  tier: TierFilter;
  status: StatusFilter;
  extension: string;
  recordType: string;
  pathPrefix: string;
  includeUnchanged: boolean;
}): DiffFilter {
  return {
    search: input.search.trim() || null,
    tiers: input.tier === "all" ? [] : [input.tier],
    statuses: input.status === "all" ? [] : [input.status],
    extensions: input.extension.trim() ? [input.extension.trim()] : [],
    record_types: input.recordType.trim() ? [input.recordType.trim()] : [],
    path_prefixes: input.pathPrefix.trim() ? [input.pathPrefix.trim()] : [],
    include_unchanged: input.includeUnchanged,
  };
}
