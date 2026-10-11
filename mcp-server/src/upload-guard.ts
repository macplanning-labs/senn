/**
 * Wiki の添付に使うローカルファイルの制限。
 *
 * 添付のツールは、AI が指定したパスのファイルを読んで SENN に送る。チケットや Wiki の文に
 * 仕込まれた指示(プロンプトインジェクション)で、AI が秘密のファイル(~/.ssh の鍵や
 * credentials.env)を添付して持ち出さないよう、送れるファイルを次に限る。
 *   - 置き場所: SENN_UPLOAD_ROOTS(OS のパス区切りで複数可)の下。未設定なら、MCP サーバーを
 *     起動したフォルダ(作業中のリポジトリ)の下。起動したフォルダが / やホームそのものなら、送れない
 *   - 途中に「.」で始まるフォルダ・ファイルを含まない(.ssh・.config・.env・.git など)
 *   - 鍵・認証情報らしい名前でない
 *   - 通常のファイルで、大きさが上限(SENN_UPLOAD_MAX_MB、既定 20MB)以下
 * シンボリックリンクは、たどった先の実際の場所で判定する。
 */
import { realpath, stat } from "node:fs/promises";
import { homedir } from "node:os";
import { basename, delimiter, isAbsolute, relative, resolve, sep } from "node:path";

const DEFAULT_MAX_MB = 20;

/** 鍵・認証情報らしいファイル名 */
const SECRET_NAME = /^(credentials\.env|id_(rsa|dsa|ecdsa|ed25519)(\.pub)?|.*\.(pem|key|p12|pfx|keystore|jks))$/i;

export type UploadCheck =
  | { ok: true; path: string; filename: string; size: number }
  | { ok: false; reason: string };

export interface UploadGuardOptions {
  roots?: string[];
  maxBytes?: number;
}

/** 環境変数から、送ってよい置き場所と上限を決める */
export function uploadOptionsFromEnv(env: NodeJS.ProcessEnv = process.env, cwd: string = process.cwd()): Required<UploadGuardOptions> {
  const configured = (env.SENN_UPLOAD_ROOTS ?? "").split(delimiter).map((s) => s.trim()).filter(Boolean);
  let roots: string[];
  if (configured.length > 0) {
    roots = configured.map((r) => resolve(r));
  } else {
    const c = resolve(cwd);
    roots = c === resolve("/") || c === resolve(homedir()) ? [] : [c];
  }
  const mb = Number(env.SENN_UPLOAD_MAX_MB);
  const maxBytes = (Number.isFinite(mb) && mb > 0 ? mb : DEFAULT_MAX_MB) * 1024 * 1024;
  return { roots, maxBytes };
}

export async function checkUploadPath(filePath: string, opts: UploadGuardOptions = {}): Promise<UploadCheck> {
  const { roots, maxBytes } = { ...uploadOptionsFromEnv(), ...opts };
  if (!isAbsolute(filePath)) {
    return { ok: false, reason: "file_path は絶対パスで指定してください。" };
  }
  if (roots.length === 0) {
    return {
      ok: false,
      reason:
        "添付に使えるフォルダが決まっていません。MCP の設定で、環境変数 SENN_UPLOAD_ROOTS に添付してよいフォルダを指定してください。",
    };
  }

  let real: string;
  try {
    real = await realpath(filePath);
  } catch {
    return { ok: false, reason: `ファイルが見つかりません、または読み取れません: ${filePath}` };
  }

  const realRoots: string[] = [];
  for (const r of roots) {
    try {
      realRoots.push(await realpath(r));
    } catch {
      // 存在しない置き場所は無視する
    }
  }
  const inside = realRoots.some((root) => {
    const rel = relative(root, real);
    return rel !== "" && !rel.startsWith("..") && !isAbsolute(rel);
  });
  if (!inside) {
    return {
      ok: false,
      reason: `このファイルは、添付してよいフォルダ(SENN_UPLOAD_ROOTS、未設定なら作業中のフォルダ)の外にあります: ${filePath}`,
    };
  }

  if (real.split(sep).some((seg) => seg.startsWith("."))) {
    return { ok: false, reason: "「.」で始まるフォルダ・ファイル(.ssh・.config・.env など)の中身は添付できません。" };
  }
  const filename = basename(real);
  if (SECRET_NAME.test(filename)) {
    return { ok: false, reason: "鍵・認証情報らしいファイルは添付できません。" };
  }

  const st = await stat(real);
  if (!st.isFile()) {
    return { ok: false, reason: "通常のファイルだけを添付できます。" };
  }
  if (st.size > maxBytes) {
    return {
      ok: false,
      reason: `ファイルが大きすぎます(上限 ${Math.round(maxBytes / 1024 / 1024)}MB。SENN_UPLOAD_MAX_MB で変えられます)。`,
    };
  }
  return { ok: true, path: real, filename, size: st.size };
}
