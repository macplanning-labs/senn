#!/usr/bin/env python3
"""pg_dump --schema-only の出力から、指定テーブルに関わる文をブロック単位で除去する。

ベースラインマイグレーション(migrations/*_initial_schema.sql)を本番DBの
スキーマから再生成する際に使う。手順は rust/scripts/README_baseline_schema.md
を参照。

使い方:
    python3 clean_schema_dump.py schema_raw.sql schema_cleaned.sql
"""
import re
import sys

# Django/django-axes由来で、Rustアプリからは一切参照されない管理テーブル。
# 新しいテーブルが増えた場合はここに追記して再実行する。
EXCLUDE_TABLES = {
    "django_migrations", "django_content_type", "django_admin_log", "django_session",
    "auth_group", "auth_group_permissions", "auth_permission",
    "axes_accessattempt", "axes_accessattemptexpiration", "axes_accessfailurelog", "axes_accesslog",
    "accounts_user_groups", "accounts_user_user_permissions",
    "_sqlx_migrations",  # sqlxが自前で作成するため、手動マイグレーションに含めない
}

EXCLUDE_SEQUENCES = {f"{t}_id_seq" for t in EXCLUDE_TABLES}


def mentions_excluded(stmt: str) -> bool:
    for t in EXCLUDE_TABLES | EXCLUDE_SEQUENCES:
        # public.table_name / "table_name" / 単体table_name の形を許容しつつ、
        # 識別子境界を要求することで auth_group が auth_group_permissions に
        # 誤マッチしないようにする。
        pattern = r'(?:public\.)?"?' + re.escape(t) + r'"?(?=[\s(,;.\n]|$)'
        if re.search(pattern, stmt):
            return True
    return False


def main():
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <input.sql> <output.sql>", file=sys.stderr)
        sys.exit(1)
    infile, outfile = sys.argv[1], sys.argv[2]

    with open(infile, "r", encoding="utf-8") as f:
        content = f.read()

    # pg_dump 16以降が埋め込む \restrict / \unrestrict は psql専用メタコマンドで
    # SQLではない(sqlxのmigratorが解釈できずエラーになる)。
    content = re.sub(r"^\\restrict .*\n", "", content, flags=re.MULTILINE)
    content = re.sub(r"^\\unrestrict .*\n", "", content, flags=re.MULTILINE)

    # セッション全体(トランザクション局所ではない)のsearch_pathを空にする文。
    # DDLは全てpublic.修飾済みなので不要だが、残すとsqlx自身の非修飾クエリ
    # (_sqlx_migrations等)が解決できなくなり migrate 失敗の原因になる。
    content = re.sub(
        r"^SELECT pg_catalog\.set_config\('search_path', '', false\);\n",
        "", content, flags=re.MULTILINE,
    )

    # ステートメント単位(";\n"区切り)に分割して判定する
    parts = re.split(r"(;\n)", content)
    statements = []
    buf = ""
    for part in parts:
        buf += part
        if part == ";\n":
            statements.append(buf)
            buf = ""
    if buf:
        statements.append(buf)

    kept = []
    removed_count = 0
    for stmt in statements:
        code_only = "\n".join(
            line for line in stmt.split("\n") if not line.strip().startswith("--")
        )
        if code_only.strip() == "":
            kept.append(stmt)
            continue
        if mentions_excluded(code_only):
            removed_count += 1
            continue
        kept.append(stmt)

    result = re.sub(r"\n{3,}", "\n\n", "".join(kept))

    with open(outfile, "w", encoding="utf-8") as f:
        f.write(result)

    print(f"除去したステートメント数: {removed_count}")
    print(f"残ったステートメント数: {len(kept)}")


if __name__ == "__main__":
    main()
