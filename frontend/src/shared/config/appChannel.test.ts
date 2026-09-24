import { describe, expect, it, vi } from 'vitest';

describe('appChannel', () => {
  it('defaults to internal when VITE_APP_CHANNEL is unset', async () => {
    vi.resetModules();
    vi.stubEnv('VITE_APP_CHANNEL', '');
    const mod = await import('./appChannel');
    expect(mod.APP_CHANNEL).toBe('internal');
    expect(mod.isDemoEntryEnabled()).toBe(false);
  });

  it('enables demo entry on customer channel', async () => {
    vi.resetModules();
    vi.stubEnv('VITE_APP_CHANNEL', 'customer');
    const mod = await import('./appChannel');
    expect(mod.APP_CHANNEL).toBe('customer');
    expect(mod.isDemoEntryEnabled()).toBe(true);
  });
});
