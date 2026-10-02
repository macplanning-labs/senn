import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

// テスト環境は node のため、DOM ではなくサーバーレンダリングした HTML を検証する
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key, i18n: { language: 'ja', changeLanguage: vi.fn() } }),
}));
vi.mock('react-router-dom', () => ({
  useParams: () => ({ projectKey: 'SALES' }),
  useNavigate: () => vi.fn(),
  Link: () => null,
}));
vi.mock('./ProjectSettings.css', () => ({}));

let mockUser: { id: number; isSystemAdmin: boolean; isGuest: boolean } | null = null;
vi.mock('@/shared/stores/authStore', () => {
  const useAuthStore = (sel?: (s: { user: typeof mockUser }) => unknown) =>
    sel ? sel({ user: mockUser }) : { user: mockUser };
  return { useAuthStore };
});
vi.mock('@/shared/stores/uiStore', () => ({ useUIStore: () => ({ theme: 'dark', toggleTheme: vi.fn() }) }));
vi.mock('@/shared/stores/toastStore', () => ({ useToastStore: () => ({ addToast: vi.fn() }) }));
vi.mock('@tanstack/react-query', () => ({ useMutation: () => ({ mutate: vi.fn(), isPending: false }) }));
vi.mock('@/shared/api/client', () => ({ apiClient: {} }));
vi.mock('@/shared/sync/projectWrites', () => ({ localUpdateProject: vi.fn() }));
vi.mock('@/shared/sync/syncEngine', () => ({ runCycle: vi.fn() }));
vi.mock('@/shared/sync/repos/projectRepo', () => ({
  useProjectByPrefix: () => ({
    id: 1, name: '営業案件', prefix: 'SALES', description: '', priority: 'medium', teams: [],
    ownerId: 99, cycleAutoComplete: true, cycleAutoCreateNext: true,
  }),
}));
vi.mock('@/features/projects/components/ProjectTeamsSection', () => ({ ProjectTeamsSection: () => null }));
vi.mock('./LabelSettings', () => ({ LabelSettings: () => null }));
vi.mock('./CategorySettings', () => ({ CategorySettings: () => null }));
vi.mock('./MilestoneSettings', () => ({ MilestoneSettings: () => null }));
vi.mock('./WorkflowSettings', () => ({ WorkflowSettings: () => null }));
vi.mock('./IntegrationSettings', () => ({ IntegrationSettings: () => null }));
vi.mock('./ChatIntegrationSettings', () => ({ ChatIntegrationSettings: () => null }));
vi.mock('./SecuritySettings', () => ({ SecuritySettings: () => null }));
vi.mock('./HolidaySettings', () => ({ HolidaySettings: () => null }));

import { ProjectSettingsPage } from './ProjectSettingsPage';

const render = () => renderToStaticMarkup(createElement(ProjectSettingsPage));
const fieldsets = (html: string) => html.match(/<fieldset[^>]*>/g) ?? [];

describe('ProjectSettingsPage', () => {
  beforeEach(() => {
    mockUser = { id: 1, isSystemAdmin: false, isGuest: false };
  });

  it('Guest には閲覧のみの案内を出し、設定の欄をまとめて無効にし、連携のタブを出さない', () => {
    mockUser = { id: 3, isSystemAdmin: false, isGuest: true };
    const html = render();
    expect(html).toContain('project-settings-readonly');
    const fs = fieldsets(html);
    expect(fs.length).toBeGreaterThan(0);
    expect(fs.every((f) => f.includes('disabled'))).toBe(true);
    expect(html).not.toContain('tab-integrations');
    // 本人の設定(言語)は無効にしない
    expect(html).toContain('settings.language');
  });

  it('Guest でなければ、設定の欄は有効で、連携のタブも出る', () => {
    const html = render();
    expect(html).not.toContain('project-settings-readonly');
    expect(fieldsets(html).some((f) => f.includes('disabled'))).toBe(false);
    expect(html).toContain('tab-integrations');
  });
});
