import type { IssueSummary } from "../types";

export function renderIssueList(
  issues: IssueSummary[],
  onCollapse?: () => void,
): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "issue-list";

  // Header row with title and collapse button
  const header = document.createElement("div");
  header.className = "issue-list__header";

  const title = document.createElement("span");
  title.className = "issue-list__title";
  title.textContent = `${issues.length} resolved issue${issues.length === 1 ? "" : "s"}`;

  const collapseBtn = document.createElement("button");
  collapseBtn.className = "issue-list__collapse-btn icon-btn";
  collapseBtn.title = "Collapse";
  collapseBtn.textContent = "▲";
  collapseBtn.dataset.tauriDragRegion = "false";
  if (onCollapse) collapseBtn.addEventListener("click", onCollapse);

  header.append(title, collapseBtn);
  wrap.append(header);

  if (issues.length === 0) {
    const empty = document.createElement("div");
    empty.className = "issue-list__empty";
    empty.textContent = "No issues resolved in this period.";
    wrap.append(empty);
    return wrap;
  }

  for (const issue of issues) {
    const row = document.createElement("div");
    row.className = "issue-row";

    const key = document.createElement("span");
    key.className = "issue-row__key";
    key.textContent = issue.key;

    const summary = document.createElement("span");
    summary.className = "issue-row__sum";
    summary.title = issue.summary;
    summary.textContent = issue.summary;

    const pts = document.createElement("span");
    pts.className = "issue-row__pts";
    pts.textContent = issue.points ? issue.points.toString() : "—";

    row.append(key, summary, pts);
    wrap.append(row);
  }
  return wrap;
}
