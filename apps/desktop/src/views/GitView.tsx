import { GitBranch, GitFork, GitPullRequestClosed, Recycle, FolderGit2 } from "lucide-react";
import type { Candidate } from "../types";
import { actionRepo } from "../types";
import { compactPath, humanSize } from "../format";
import type { Lang } from "../i18n";
import { STRINGS } from "../i18n";
import { CheckBox, RiskPill, Tags } from "../components/common";

interface Props {
  lang: Lang;
  home: string;
  candidates: Candidate[];
  excluded: Set<string>;
  selected: Set<string>;
  gitEnabled: boolean;
  onToggle: (id: string) => void;
  onActivate: (id: string) => void;
}

export function GitView({ lang, home, candidates, excluded, selected, gitEnabled, onToggle, onActivate }: Props) {
  const s = STRINGS[lang];
  const git = candidates.filter((c) => c.category === "git" && !excluded.has(c.id));
  if (!gitEnabled) return <div className="panel"><div className="empty"><span className="ei"><GitBranch size={24} /></span><strong>{s.gitDisabled}</strong></div></div>;
  if (git.length === 0) return <div className="panel"><div className="empty"><span className="ei"><GitBranch size={24} /></span><strong>{s.gitEmpty}</strong></div></div>;
  const repos = new Map<string, Candidate[]>();
  for (const c of git) {
    const repo = actionRepo(c.action) ?? "?";
    if (!repos.has(repo)) repos.set(repo, []);
    repos.get(repo)!.push(c);
  }
  const icon = (c: Candidate) => {
    switch (c.action.type) {
      case "git-gc": return <Recycle size={15} />;
      case "git-branch-delete": return <GitPullRequestClosed size={15} />;
      default: return <GitFork size={15} />;
    }
  };
  return (
    <>
      {[...repos.entries()].map(([repo, items]) => (
        <section className="panel repo" key={repo}>
          <div className="repo-h">
            <span className="ri"><FolderGit2 size={15} /></span>
            <strong>{repo.split(/[\\/]/).pop()}</strong>
            <code>{compactPath(repo, home)}</code>
            <span className="sum">{humanSize(items.reduce((a, c) => a + c.size, 0))}</span>
          </div>
          {items.map((c) => (
            <div key={c.id} className={`grow ${selected.has(c.id) ? "sel" : ""}`} onClick={() => onToggle(c.id)} onDoubleClick={() => onActivate(c.id)}>
              <span><CheckBox checked={selected.has(c.id)} /></span>
              <span className="gi">{icon(c)}</span>
              <div className="cell">
                <span className="n">{c.label} <Tags c={c} lang={lang} /></span>
                <span className="p">{c.description}</span>
              </div>
              <RiskPill risk={c.risk} lang={lang} />
              <span className="size r">{c.size_exact ? "" : <span className="est">≈ </span>}{c.size ? humanSize(c.size) : "—"}</span>
            </div>
          ))}
        </section>
      ))}
    </>
  );
}
