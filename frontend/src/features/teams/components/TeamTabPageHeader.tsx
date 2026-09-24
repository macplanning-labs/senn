import type { FC, ReactNode } from 'react';
import './TeamTabPageHeader.css';

export function TeamTabPageHeader({
  icon: Icon,
  title,
  actions,
  subtitle,
}: {
  icon: FC;
  title: string;
  actions?: ReactNode;
  subtitle?: ReactNode;
}) {
  return (
    <div className="team-tab-page__header">
      <div className="team-tab-page__heading">
        <h1 className="team-tab-page__title">
          <span className="team-tab-page__icon" aria-hidden="true"><Icon /></span>
          {title}
        </h1>
        {subtitle}
      </div>
      {actions ? <div className="team-tab-page__actions">{actions}</div> : null}
    </div>
  );
}
