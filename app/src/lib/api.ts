// Typed client for the app's Rust commands (app/src-tauri/src/commands.rs, ADR-0004).
// The Rust side owns the open project; the UI asks for one page of rows at a time and
// keeps no copy of project data.
import { invoke } from '@tauri-apps/api/core';

// @implements CT-EFF-005
/** The most rows one page holds, as `MAX_PAGE` in app/src-tauri/src/data.rs. */
export const MAX_PAGE = 200;

// @implements CT-USE-007
/** What went wrong, where (a file and line, a folder, or empty), and how to fix it. */
export interface Problem {
  what: string;
  where: string;
  help: string;
}

// @implements CT-USE-007
/** A command error. Its message joins what, where, and how to fix it. */
export class ClewError extends Error {
  readonly problem: Problem;

  constructor(problem: Problem) {
    const place = problem.where ? `${problem.where}: ` : '';
    super(`${place}${problem.what}. ${problem.help}`);
    this.name = 'ClewError';
    this.problem = problem;
  }
}

// @implements CT-EFF-005
/** One page of rows. `total` is null when counting would cost a second query. */
export interface Page<Row> {
  offset: number;
  total: number | null;
  more: boolean;
  rows: Row[];
}

// @implements CT-TRC-004
/** The coverage filters, as the Rust side names them (`GAP_FILTERS` in data.rs). */
export type GapFilter = 'no-parent' | 'no-implementation' | 'no-verification' | 'untraced';

// @implements CT-TRC-004, CT-EFF-005
/** An element a coverage filter lists. */
export interface GapRow {
  id: number;
  kind: string;
  name: string | null;
  shortName: string | null;
  qualifiedName: string;
  file: string;
  line: number;
}

// @implements CT-TRC-004, CT-EFF-005
/** One line of a project file in the source view. */
export interface SourceLine {
  line: number;
  text: string;
}

// @implements CT-EFF-004
/** The counts of an open project. `search` is false when its index could not be opened. */
export interface ProjectSummary {
  root: string;
  elements: number;
  requirements: number;
  links: number;
  problems: number;
  warnings: number;
  search: boolean;
  /** How many elements each coverage filter lists. */
  gaps: Record<GapFilter, number>;
}

// @implements CT-EFF-004, CT-USE-007
export type ProjectStatus =
  | { state: 'none' }
  | { state: 'loading'; path: string }
  | { state: 'ready'; project: ProjectSummary }
  | { state: 'failed'; error: Problem };

// @implements CT-EFF-005
/** An element in the model tree. Element IDs last until the project is opened again. */
export interface TreeRow {
  id: number;
  kind: string;
  name: string | null;
  shortName: string | null;
  children: number;
  file: string;
  line: number;
}

// @implements CT-EFF-005
/** The text and coverage of a requirement with an ID. */
export interface RequirementDetail {
  doc: string;
  verificationMethod: string | null;
  phase: number | null;
  hasParent: boolean;
  implemented: boolean;
  verified: boolean;
  waived: boolean;
}

// @implements CT-EFF-005
export interface ElementDetail {
  id: number;
  kind: string;
  name: string | null;
  shortName: string | null;
  qualifiedName: string;
  file: string;
  line: number;
  column: number;
  owner: number | null;
  children: number;
  linksFrom: number;
  linksTo: number;
  requirement: RequirementDetail | null;
}

// @implements CT-EFF-005
/** The far end of a link: a model element, or the lines of a tagged code scope. */
export type LinkEnd =
  | { id: number; kind: string; name: string }
  | { path: string; startLine: number; endLine: number };

// @implements CT-EFF-005
/** A trace link, such as `satisfies`, and the place in the text that declares it. */
export interface LinkRow {
  kind: string;
  other: LinkEnd;
  file: string;
  line: number;
}

// @implements CT-TRC-005, CT-EFF-005
/** An end reached by following links, how many links away it is, and the link that reached it. */
export interface ImpactRow extends LinkRow {
  depth: number;
}

// @implements CT-EFF-005, CT-USE-007
export interface ProblemRow extends Problem {
  severity: 'error' | 'warning';
}

// @implements CT-EFF-005, CT-EFF-006
/** A search hit. `id` is null when the hit is in a file the model could not read. */
export interface SearchHit {
  id: number | null;
  kind: string;
  qualifiedName: string;
  shortName: string | null;
  file: string;
  line: number;
  column: number;
}

// @begin CT-USE-007, CT-EFF-005
// What every command but the status ones replies.
type Reply<T> = { ok: T } | { error: Problem };
// @end

// @implements CT-USE-007, CT-EFF-005
/** Calls a command that replies `{ok}` or `{error}`, and throws a ClewError on error. */
async function call<T>(command: string, args: Record<string, unknown>): Promise<T> {
  const reply = await invoke<Reply<T>>(command, args);
  if ('error' in reply) {
    throw new ClewError(reply.error);
  }
  return reply.ok;
}

// @implements CT-EFF-004
/** Whether a project is open, loading, or failed to open. */
export function projectStatus(): Promise<ProjectStatus> {
  return invoke<ProjectStatus>('project_status');
}

// @implements CT-EFF-004
/** Resolves when a load in progress ends, with the status it ended in. */
export function waitForProject(): Promise<ProjectStatus> {
  return invoke<ProjectStatus>('wait_for_project');
}

// @implements CT-EFF-005
/** One page of the children of element `parent` in the model tree; 0 is the root. */
export function treeChildren(parent: number, offset = 0, limit = MAX_PAGE): Promise<Page<TreeRow>> {
  return call('tree_children', { parent, offset, limit });
}

// @implements CT-EFF-005
export function elementDetail(id: number): Promise<ElementDetail> {
  return call('element_detail', { id });
}

// @implements CT-EFF-005
/** One page of the links that start at (`from`) or end at (`to`) an element. */
export function elementLinks(
  id: number,
  direction: 'from' | 'to',
  offset = 0,
  limit = MAX_PAGE,
): Promise<Page<LinkRow>> {
  return call('element_links', { id, direction, offset, limit });
}

// @implements CT-TRC-005, CT-EFF-005
/** One page of what is reachable `upstream` or `downstream` of an element, nearest first. */
export function elementImpact(
  id: number,
  direction: 'upstream' | 'downstream',
  offset = 0,
  limit = MAX_PAGE,
): Promise<Page<ImpactRow>> {
  return call('element_impact', { id, direction, offset, limit });
}

// @implements CT-TRC-004, CT-EFF-005
/** One page of the elements a coverage filter lists. */
export function coverageGaps(filter: GapFilter, offset = 0, limit = MAX_PAGE): Promise<Page<GapRow>> {
  return call('coverage_gaps', { filter, offset, limit });
}

// @implements CT-TRC-004, CT-EFF-005
/** One page of the lines of a project file, by a path relative to the project folder. */
export function sourceLines(path: string, offset = 0, limit = MAX_PAGE): Promise<Page<SourceLine>> {
  return call('source_lines', { path, offset, limit });
}

// @implements CT-EFF-005, CT-USE-007
/** One page of the project's errors, then its warnings. */
export function problems(offset = 0, limit = MAX_PAGE): Promise<Page<ProblemRow>> {
  return call('problems', { offset, limit });
}

// @implements CT-EFF-005, CT-EFF-006
/** One page of full-text search hits, best first. `total` is always null. */
export function search(text: string, offset = 0, limit = MAX_PAGE): Promise<Page<SearchHit>> {
  return call('search', { text, offset, limit });
}

// @implements CT-SEC-003
/** A recent project: the folder path and its name. Paths only, never project content. */
export interface RecentProject {
  path: string;
  name: string;
}

// @implements CT-SEC-003
/** Shows the OS folder dialog. Resolves with the chosen folder, or null when cancelled. */
export function pickFolder(): Promise<string | null> {
  return call('pick_folder', {});
}

// @implements CT-SEC-003, CT-USE-007
/** Opens the project in `path`; resolves with the status once its load ends. */
export function openProject(path: string): Promise<ProjectStatus> {
  return invoke<ProjectStatus>('open_project', { path });
}

// @implements CT-SEC-003
/** The recent projects, newest first. */
export function recentProjects(): Promise<RecentProject[]> {
  return call('recent_projects', {});
}

// @implements CT-SEC-003
/** Empties the recent list. */
export function clearRecent(): Promise<void> {
  return call<unknown[]>('clear_recent', {}).then(() => undefined);
}

// @implements CT-EFF-004, CT-USE-007
/** One line describing a project status, for the window until the app shell exists. */
export function describeStatus(status: ProjectStatus): string {
  switch (status.state) {
    case 'none':
      return 'No project open. Start Clewtrace with a project folder.';
    case 'loading':
      return `Opening ${status.path}`;
    case 'ready': {
      const p = status.project;
      return `${p.root}: ${p.requirements} requirements, ${p.links} links, ${p.problems} problems`;
    }
    case 'failed':
      return new ClewError(status.error).message;
  }
}
