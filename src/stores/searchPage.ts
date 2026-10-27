// 反查与全文页的输入和方式：查词页的反查条点「全部」时带着查询切过去。

import { create } from "zustand";

export type SearchMode = "reverse" | "text";

export const useSearchPage = create<{
  query: string;
  mode: SearchMode;
  setQuery: (query: string) => void;
  setMode: (mode: SearchMode) => void;
}>((set) => ({
  query: "",
  mode: "reverse",
  setQuery: (query) => set({ query }),
  setMode: (mode) => set({ mode }),
}));
