import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  name: string;
  version: string;
  stage: "core";
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}

export interface Overview {
  entries: number;
  options: number;
  linux_modules: number;
  areas: number;
  profiles: number;
  schema_version: number;
  database_path: string;
}
export interface SearchResult {
  visual: boolean;
  id: string;
  title: string;
  kind: string;
  area_id: string;
  area_name: string;
  module_id: string;
  module_name: string;
  snippet: string;
  match_kind: "exact" | "textual" | "approximate";
  favorite: boolean;
  learning_status: string;
}
export interface EntryIndex {
  id: string;
  title: string;
  module_name: string;
}
export interface CommandOption {
  id: string;
  example: string;
  nomenclature?: string;
  description?: string;
  application?: string;
  featured?: boolean;
}
export interface EntryDetail {
  id: string;
  area_name: string;
  module_name: string;
  payload: {
    title: string;
    type: string;
    full_name?: string;
    technical_description?: string;
    simplified_description?: string;
    personal_description?: string;
    options?: CommandOption[];
    learning_status?: string;
    options_coverage?: string;
    sources?: string[];
    risk?: string;
    module_id: string;
    keywords?: string[];
    related_entries?: string[];
    markdown?: string;
    platform?: string;
  };
  revision: number;
  favorite: boolean;
  area_id: string;
  module_id: string;
  presentation: Presentation;
  blocks: BlockView[];
}
export interface BlockPayload {
  id: string;
  type: string;
  title?: string;
  content?: string;
  format?: string;
  language?: string;
  caption?: string;
  keywords?: string[];
  asset_id?: string;
  preview_asset_id?: string;
  items?: string[];
  columns?: string[];
  rows?: string[][];
  nodes?: unknown[];
  edges?: unknown[];
  options?: CommandOption[];
  coverage?: string;
  risk?: string;
}
export interface BlockView {
  payload: BlockPayload;
  virtual_block: boolean;
}
export interface Presentation {
  profile_id: string;
  source: string;
  components: string[];
}
export interface Area {
  id: string;
  name: string;
  profile_id?: string;
  enabled_components?: string[];
  entry_count: number;
  revision: number;
}
export interface Module {
  id: string;
  name: string;
  area_id: string;
  parent_id?: string | null;
  profile_id?: string;
  enabled_components?: string[];
  revision: number;
  entry_count: number;
}
export interface Profile {
  id: string;
  name: string;
  enabled_components: string[];
}
export interface Catalog {
  areas: Area[];
  modules: Module[];
  profiles: Profile[];
  components: string[];
}
export interface CatalogFilter {
  area_id: string | null;
  module_id: string | null;
  entry_type: string | null;
  learning_status: string | null;
  favorites_only: boolean;
}
export interface CatalogPage {
  items: SearchResult[];
  total: number;
}
export interface History {
  visits: { entry_id: string; title: string; at: string }[];
  searches: { query: string; at: string }[];
}
export type TermKind = "synonym" | "personal";
export interface VocabularyTerm {
  id: number;
  kind: TermKind;
  phrase: string;
  target_entry_id: string;
  target_title: string;
}
export interface TermInput {
  id: number | null;
  kind: TermKind;
  phrase: string;
  target_entry_id: string;
}

export const getOverview = (): Promise<Overview> => invoke("get_overview");
export const searchKnowledge = (
  query: string,
  filters?: CatalogFilter,
): Promise<SearchResult[]> =>
  invoke("search_knowledge", {
    request: { query, limit: 100, filters: filters ?? null },
  });
export const getEntry = (id: string): Promise<EntryDetail> =>
  invoke("get_entry", { id });
export const getEntriesIndex = (): Promise<EntryIndex[]> =>
  invoke("get_entries_index");
export const listVocabulary = (): Promise<VocabularyTerm[]> =>
  invoke("list_vocabulary");
export const saveVocabularyTerm = (input: TermInput): Promise<number> =>
  invoke("save_vocabulary_term", { input });
export const deleteVocabularyTerm = (
  kind: TermKind,
  id: number,
): Promise<void> => invoke("delete_vocabulary_term", { kind, id });
export const getCatalog = (): Promise<Catalog> => invoke("get_catalog");
export const resolvePresentation = (moduleId: string): Promise<Presentation> =>
  invoke("resolve_presentation", { moduleId });
export const savePresentation = (input: {
  module_id: string;
  expected_revision: number;
  profile_id: string | null;
  enabled_components: string[] | null;
}): Promise<void> => invoke("save_presentation", { input });
export const createProfile = (json: string): Promise<void> =>
  invoke("create_profile", { json });
export const createArea = (input: {
  id: string;
  name: string;
  profile_id: string | null;
}): Promise<void> => invoke("create_area", { input });
export const createModule = (input: {
  id: string;
  name: string;
  area_id: string;
  parent_id: string | null;
  profile_id: string | null;
}): Promise<void> => invoke("create_module", { input });
export const browseEntries = (
  filters: CatalogFilter,
  offset = 0,
): Promise<CatalogPage> =>
  invoke("browse_entries", { request: { filters, offset } });
export const setFavorite = (id: string, value: boolean): Promise<void> =>
  invoke("set_favorite", { id, value });
export const getHistory = (): Promise<History> => invoke("get_history");
export const saveEntry = (input: {
  id: string;
  expected_revision: number | null;
  patch: Record<string, unknown>;
  confirm_personal: boolean;
}): Promise<EntryDetail> => invoke("save_entry", { input });
export const saveBlock = (input: {
  entry_id: string;
  expected_revision: number;
  patch: unknown;
  confirm_personal: boolean;
}): Promise<EntryDetail> => invoke("save_block", { input });
export const reorderBlocks = (input: {
  entry_id: string;
  expected_revision: number;
  ids: string[];
}): Promise<EntryDetail> => invoke("reorder_blocks", { input });
