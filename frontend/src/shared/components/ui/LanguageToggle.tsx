/**
 * LanguageToggle.tsx — 言語切替ボタン(日本語 ⇄ 英語)
 *
 * ログイン後の画面(MainLayout)と、ログイン前の認証画面(ログイン・新規登録・
 * パスワード再設定)の両方で使う。選んだ言語はブラウザに保存される(changeAppLanguage)。
 */
import { useTranslation } from 'react-i18next';
import { changeAppLanguage } from '@/i18n';
import './LanguageToggle.css';

interface LanguageToggleProps {
  /** 追加のクラス名(配置やテスト用の識別に使う) */
  className?: string;
}

export function LanguageToggle({ className = '' }: LanguageToggleProps) {
  const { t, i18n } = useTranslation();
  const isJa = i18n.language === 'ja';
  const label = isJa ? t('app.switchToEnglish') : t('app.switchToJapanese');
  return (
    <button
      type="button"
      className={`lang-toggle ${className}`.trim()}
      onClick={() => changeAppLanguage(isJa ? 'en' : 'ja')}
      title={label}
      aria-label={label}
      data-testid="language-toggle"
    >
      {isJa ? '🇯🇵' : '🇺🇸'}
    </button>
  );
}
