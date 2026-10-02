//! domain/access — アクセス制御の規則(唯一の置き場所)
//!
//! 設計: docs/design/設計書_アクセス制御_再設計.md / 詳細設計書_アクセス制御_再設計.md
//!
//! ここは DB に触らない純粋な規則だけを持つ。
//! - `Viewer`   : いま操作している主体(人・個人キー・外部連携)と、その役割・有効な所属
//! - `ResourceRef` : 判定に必要な、リソースの所属(本文は持たない)
//! - `policy::can` : 「この閲覧者は、このリソースに、この操作をしてよいか」
//!
//! 見え方の規則を、ハンドラやリポジトリに書かない。必ずここを通す(規則の重複を作らない)。

// フェーズ B(土台)の時点では、判定・絞り込みはテストと抽出器からだけ使う。ハンドラ・リポジトリから使い始める
// フェーズ C で、この抑止を外す(タスクリスト C-1)。
#![allow(dead_code)]

pub mod policy;
pub mod resource;
pub mod scope;
pub mod system;
pub mod viewer;

#[cfg(test)]
mod matrix_tests;

// 再公開は、フェーズ C 以降でハンドラ・リポジトリから使う(それまでは未使用の警告を抑止する)
#[allow(unused_imports)]
pub use policy::{can, manages_owners, manages_team, sees_team, Action, Decision};
#[allow(unused_imports)]
pub use resource::{ResourceRef, SettingsPolicy, TeamFacts, Visibility, WikiScope};
#[allow(unused_imports)]
pub use scope::Scope;
#[allow(unused_imports)]
pub use system::SystemContext;
pub use viewer::{Membership, MembershipKind, Principal, Role, Viewer};
