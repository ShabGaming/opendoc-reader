import type { MouseEvent } from "react";
import { t, tp } from "../../i18n";
import { useApp } from "../../state/AppContext";
import styles from "./Contributors.module.css";
import { avatarUrl, type Contributor } from "./contributors";

function Avatar({ contributor }: { contributor: Contributor }) {
  const src = avatarUrl(contributor.avatar);
  const label = contributor.name ?? contributor.login;
  if (src) {
    return <img className={styles.avatar} src={src} width={32} height={32} alt={label} />;
  }
  return (
    <span className={styles.initials} role="img" aria-label={label}>
      {label.slice(0, 1).toUpperCase()}
    </span>
  );
}

/** The people who contributed, from the list bundled at build time. */
export function ContributorList({ contributors }: { contributors: Contributor[] }) {
  const { platform } = useApp();
  return (
    <ul className={styles.list} aria-label={t("settings.about.contributorsList")}>
      {contributors.map((c) => {
        const name = c.name ?? c.login;
        return (
          <li key={c.login} className={styles.item}>
            <Avatar contributor={c} />
            <div className={styles.text}>
              <a
                href={c.profileUrl}
                aria-label={t("settings.about.openProfile", { name })}
                onClick={(event: MouseEvent) => {
                  event.preventDefault();
                  void platform.openUrl(c.profileUrl);
                }}
              >
                {name}
              </a>
              <span className={styles.meta}>
                {c.name ? `@${c.login} · ` : ""}
                {tp("settings.about.contributions", c.contributions)}
              </span>
            </div>
          </li>
        );
      })}
    </ul>
  );
}
