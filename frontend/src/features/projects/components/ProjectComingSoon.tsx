/**
 * ProjectComingSoon.tsx - 準備中プレースホルダー
 *
 * Activity/Projects タブなど未実装機能の仮表示
 */

import { useTranslation } from 'react-i18next';
import './ProjectComingSoon.css';

interface ProjectComingSoonProps {
  messageKey: string;
  testId: string;
}

export function ProjectComingSoon({ messageKey, testId }: ProjectComingSoonProps) {
  const { t } = useTranslation();

  return (
    <div className="project-coming-soon" data-testid={testId}>
      <div className="project-coming-soon__content">
        <p className="project-coming-soon__message">{t(messageKey)}</p>
      </div>
    </div>
  );
}
