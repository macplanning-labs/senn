/**
 * DemoReadyMessage.tsx — サンプルデータ生成完了メッセージ
 */

import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';

interface Props {
  prefix: string;
}

export function DemoReadyMessage({ prefix }: Props) {
  const { t } = useTranslation();

  return (
    <div className="demo-ready-message">
      <p className="demo-ready-message__text">{t('onboarding.demoReady')}</p>
      <div className="demo-ready-message__links">
        <Link
          to={`/project/${prefix}/gantt`}
          className="demo-ready-message__link demo-ready-message__link--primary"
        >
          {t('onboarding.openGantt')}
        </Link>
        <Link
          to={`/project/${prefix}/dependencies`}
          className="demo-ready-message__link"
        >
          {t('onboarding.openDependencies')}
        </Link>
        <Link
          to={`/project/${prefix}/tickets`}
          className="demo-ready-message__link"
        >
          {t('onboarding.openTickets')}
        </Link>
      </div>
    </div>
  );
}
