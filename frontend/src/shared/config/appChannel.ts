/**
 * appChannel — ビルドチャネル（customer / internal）
 *
 * 正本: docs/方針_ビルドチャネル_customerとinternal.md
 * 未設定は internal（自社向け既定。デモ入口を誤って出さない）。
 */

export type AppChannel = 'customer' | 'internal';

function readChannel(): AppChannel {
  const raw = (import.meta.env.VITE_APP_CHANNEL as string | undefined)?.trim().toLowerCase();
  if (raw === 'customer') {
    return 'customer';
  }
  return 'internal';
}

/** ビルド時に固定されるチャネル */
export const APP_CHANNEL: AppChannel = readChannel();

export function isCustomerChannel(): boolean {
  return APP_CHANNEL === 'customer';
}

export function isInternalChannel(): boolean {
  return APP_CHANNEL === 'internal';
}

/** 登録なしデモ入口（/demo）を有効にするか */
export function isDemoEntryEnabled(): boolean {
  return isCustomerChannel();
}
