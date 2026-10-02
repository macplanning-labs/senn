//! 組み合わせの表(詳細設計書 §13.1 の DB なしの部分)
//!
//! 期待値は、設計書 §5・§6 と詳細設計書 §5 の表から、1 行ずつ書き起こしたもの。
//! 規則を変えるときは、先にこの表を変える。

use super::*;

// --- チーム ---------------------------------------------------------------------
const PUB: i32 = 1; // Public チーム
const PRIV: i32 = 2; // Private チーム
const PUB_OWNERS: i32 = 3; // Public・Owner に絞ったチーム(Owner あり)
const PUB_OWNERS_NONE: i32 = 4; // Public・Owner に絞ったチーム(Owner 0 人)
const PRIV_OWNERS: i32 = 5; // Private・Owner に絞ったチーム(Owner あり)
const PROJ: i32 = 100; // PRIV に属するプロジェクト

fn team(id: i32) -> TeamFacts {
    let (visibility, settings_policy, owner_count) = match id {
        PUB => (Visibility::Public, SettingsPolicy::Members, 0),
        PRIV => (Visibility::Private, SettingsPolicy::Members, 1),
        PUB_OWNERS => (Visibility::Public, SettingsPolicy::Owners, 1),
        PUB_OWNERS_NONE => (Visibility::Public, SettingsPolicy::Owners, 0),
        PRIV_OWNERS => (Visibility::Private, SettingsPolicy::Owners, 1),
        _ => unreachable!(),
    };
    TeamFacts {
        team_id: id,
        visibility,
        settings_policy,
        owner_count,
    }
}

// --- 閲覧者 ---------------------------------------------------------------------
fn member(team_id: i32, owner: bool) -> Membership {
    Membership {
        team_id,
        kind: MembershipKind::Team,
        is_owner: owner,
        expires_on: None,
    }
}
fn project_member(team_id: i32, project_id: i32) -> Membership {
    Membership {
        team_id,
        kind: MembershipKind::Project { project_id },
        is_owner: false,
        expires_on: None,
    }
}
fn viewer(user_id: i32, role: Role, memberships: Vec<Membership>) -> Viewer {
    Viewer {
        principal: Principal::Human { user_id },
        role,
        memberships,
        public_team_ids: vec![PUB, PUB_OWNERS, PUB_OWNERS_NONE],
    }
}

/// 閲覧者の種類(表の行の見出し)
fn v(name: &str) -> Viewer {
    match name {
        "FM未所属" => viewer(10, Role::FullMember, vec![]),
        "FM_PUB所属" => viewer(11, Role::FullMember, vec![member(PUB, false)]),
        "FM_PRIV所属" => viewer(12, Role::FullMember, vec![member(PRIV, false)]),
        "FM_PRIV_Owner" => viewer(16, Role::FullMember, vec![member(PRIV, true)]),
        "FM_OWNERS_Owner" => viewer(13, Role::FullMember, vec![member(PUB_OWNERS, true)]),
        "FM_OWNERS_一般" => viewer(14, Role::FullMember, vec![member(PUB_OWNERS, false)]),
        "FM_OWNERS_NONE_一般" => {
            viewer(15, Role::FullMember, vec![member(PUB_OWNERS_NONE, false)])
        }
        "管理者未所属" => viewer(20, Role::SystemAdmin, vec![]),
        "管理者_PRIV_OWNERS一般" => {
            viewer(21, Role::SystemAdmin, vec![member(PRIV_OWNERS, false)])
        }
        "Guest_PUB" => viewer(30, Role::Guest, vec![member(PUB, false)]),
        "Guest_PRIV" => viewer(31, Role::Guest, vec![member(PRIV, false)]),
        "Guest_なし" => viewer(32, Role::Guest, vec![]),
        "PJGuest" => viewer(33, Role::Guest, vec![project_member(PRIV, PROJ)]),
        "キー_FM_PUB所属" => Viewer {
            principal: Principal::PersonalKey {
                user_id: 11,
                key_id: 1,
            },
            ..v("FM_PUB所属")
        },
        "連携_PUB" => Viewer {
            principal: Principal::Integration { integration_id: 1 },
            role: Role::Integration,
            memberships: vec![member(PUB, false)],
            public_team_ids: vec![PUB, PUB_OWNERS, PUB_OWNERS_NONE],
        },
        _ => unreachable!("{name}"),
    }
}

// --- リソース -------------------------------------------------------------------
fn r(name: &str) -> ResourceRef {
    match name {
        "チームPUB" => ResourceRef::Team(team(PUB)),
        "チームPRIV" => ResourceRef::Team(team(PRIV)),
        "チームOWNERS" => ResourceRef::Team(team(PUB_OWNERS)),
        "チームOWNERS_NONE" => ResourceRef::Team(team(PUB_OWNERS_NONE)),
        "チームPRIV_OWNERS" => ResourceRef::Team(team(PRIV_OWNERS)),
        "チケットPUB" => ResourceRef::Ticket {
            team: Some(team(PUB)),
            project_id: None,
            author_id: Some(99),
            assignee_ids: vec![],
        },
        "チケットPUB_自作(11)" => ResourceRef::Ticket {
            team: Some(team(PUB)),
            project_id: None,
            author_id: Some(11),
            assignee_ids: vec![],
        },
        "チケットPRIV" => ResourceRef::Ticket {
            team: Some(team(PRIV)),
            project_id: None,
            author_id: Some(99),
            assignee_ids: vec![],
        },
        "チケットPRIV_PROJ" => ResourceRef::Ticket {
            team: Some(team(PRIV)),
            project_id: Some(PROJ),
            author_id: Some(99),
            assignee_ids: vec![],
        },
        "チケット_チーム無し_作成者10" => ResourceRef::Ticket {
            team: None,
            project_id: None,
            author_id: Some(10),
            assignee_ids: vec![],
        },
        "プロジェクトPRIV" => ResourceRef::Project {
            project_id: PROJ,
            teams: vec![team(PRIV)],
        },
        "プロジェクトPUB+PRIV" => ResourceRef::Project {
            project_id: 101,
            teams: vec![team(PUB), team(PRIV)],
        },
        "サイクルPUB" => ResourceRef::Cycle { team: team(PUB) },
        "サイクルPRIV" => ResourceRef::Cycle { team: team(PRIV) },
        "WikiPRIV_PROJ" => ResourceRef::Wiki {
            scope: WikiScope::Project {
                project_id: PROJ,
                teams: vec![team(PRIV)],
            },
            author_id: Some(99),
        },
        "Wiki全体" => ResourceRef::Wiki {
            scope: WikiScope::Global,
            author_id: Some(99),
        },
        "マスタ" => ResourceRef::GlobalMaster,
        "ロードマップ" => ResourceRef::Roadmap,
        "ユーザーFM" => ResourceRef::User {
            user_id: 50,
            is_guest: false,
            team_ids: vec![],
            project_grants: vec![],
        },
        "ユーザーGuest_PRIV" => ResourceRef::User {
            user_id: 51,
            is_guest: true,
            team_ids: vec![PRIV],
            project_grants: vec![],
        },
        "ユーザーGuest_PUB" => ResourceRef::User {
            user_id: 52,
            is_guest: true,
            team_ids: vec![PUB],
            project_grants: vec![],
        },
        _ => unreachable!("{name}"),
    }
}

use Action::*;
use Decision::{Allow as A, Forbidden as F, NotFound as N};

/// (閲覧者, 操作, リソース, 期待)
const TABLE: &[(&str, Action, &str, Decision)] = &[
    // ── チームの見え方(設計書 §5.1) ──
    ("FM未所属", Read, "チームPUB", A),
    ("FM未所属", Read, "チームPRIV", N), // Private は存在も見えない
    ("FM_PRIV所属", Read, "チームPRIV", A),
    ("管理者未所属", Read, "チームPUB", A),
    ("管理者未所属", Read, "チームPRIV", N), // システム管理者でも Private は見えない
    ("Guest_PUB", Read, "チームPUB", A),
    ("Guest_なし", Read, "チームPUB", N), // Guest は招待先の外を見られない
    ("Guest_PUB", Read, "チームPRIV", N),
    ("PJGuest", Read, "チームPRIV", N), // プロジェクト単位の所属では、チームは見えない
    ("連携_PUB", Read, "チームPUB", A),
    ("連携_PUB", Read, "チームOWNERS", N), // 連携は、許可されたチームだけ(Public でも)
    // ── チームの設定の管理(詳細 §5.3) ──
    ("FM_PUB所属", ManageSettings, "チームPUB", A), // members(既定): メンバー全員
    ("FM未所属", ManageSettings, "チームPUB", F),   // 未所属は不可(見えるので 403)
    ("FM_OWNERS_Owner", ManageSettings, "チームOWNERS", A),
    ("FM_OWNERS_一般", ManageSettings, "チームOWNERS", F), // Owner に絞ったチーム
    (
        "FM_OWNERS_NONE_一般",
        ManageSettings,
        "チームOWNERS_NONE",
        A,
    ), // Owner 0 人 → 全員に戻る
    ("管理者未所属", ManageSettings, "チームOWNERS", A), // システム管理者は Public の設定を管理できる(§15-11)
    ("管理者未所属", ManageSettings, "チームPRIV", N),   // Private は見えない
    ("管理者_PRIV_OWNERS一般", Read, "チームPRIV_OWNERS", A),
    (
        "管理者_PRIV_OWNERS一般",
        ManageSettings,
        "チームPRIV_OWNERS",
        F,
    ), // Private では、管理者でも所属の規則に従う
    ("Guest_PUB", ManageSettings, "チームPUB", F), // Guest は管理できない
    ("キー_FM_PUB所属", ManageSettings, "チームPUB", F), // キー経由は管理できない
    ("FM_PUB所属", ManageMembers, "チームPUB", A), // Full Member の追加・削除は方針に従う
    ("FM_OWNERS_一般", ManageMembers, "チームOWNERS", F),
    // ── Owner の操作(詳細 §5.4。方針にかかわらず Owner とシステム管理者だけ。DEMO-000169) ──
    ("FM_PUB所属", ManageOwners, "チームPUB", F), // Join しただけでは Owner を決められない
    ("FM_PUB所属", ChangeVisibility, "チームPUB", F),
    ("FM_PUB所属", Invite, "チームPUB", F), // 招待は登録の停止をすり抜けられるので Owner だけ
    ("FM_PUB所属", Delete, "チームPUB", F),
    ("FM_OWNERS_Owner", ManageOwners, "チームOWNERS", A),
    ("FM_OWNERS_Owner", ChangeVisibility, "チームOWNERS", A),
    ("FM_OWNERS_Owner", Invite, "チームOWNERS", A),
    ("FM_OWNERS_Owner", Delete, "チームOWNERS", A),
    ("FM_OWNERS_一般", ManageOwners, "チームOWNERS", F),
    ("FM_OWNERS_NONE_一般", ManageOwners, "チームOWNERS_NONE", F), // Owner 0 人でも、設定の管理と違い、全員には戻らない
    ("FM_PRIV所属", ManageOwners, "チームPRIV", F),
    ("FM_PRIV_Owner", ManageOwners, "チームPRIV", A),
    ("FM_PRIV_Owner", ChangeVisibility, "チームPRIV", A),
    ("管理者未所属", ManageOwners, "チームPUB", A), // Owner 0 人の Public でも、システム管理者は指名できる
    ("管理者未所属", ManageOwners, "チームOWNERS_NONE", A),
    ("管理者未所属", Delete, "チームPUB", A),
    ("管理者未所属", ManageOwners, "チームPRIV", N), // Private は見えない(救済は専用の API)
    (
        "管理者_PRIV_OWNERS一般",
        ManageOwners,
        "チームPRIV_OWNERS",
        A,
    ), // 所属していれば、システム管理者として Owner の操作ができる
    ("Guest_PUB", ManageOwners, "チームPUB", F),
    ("Guest_PUB", Invite, "チームPUB", F),
    ("キー_FM_PUB所属", ManageOwners, "チームPUB", F),
    ("キー_FM_PUB所属", Delete, "チームPUB", F),
    // ── Join ──
    ("FM未所属", Join, "チームPUB", A),
    ("FM_PUB所属", Join, "チームPUB", F),   // すでに所属
    ("Guest_PUB", Join, "チームOWNERS", N), // Guest は他のチームを見られない
    ("キー_FM_PUB所属", Join, "チームOWNERS", F),
    // ── チケット(設計書 §5.2・§6.1) ──
    ("FM未所属", Read, "チケットPUB", A),
    ("FM未所属", Write, "チケットPUB", A), // 未 Join でも書き込める
    ("FM未所属", Read, "チケットPRIV", N),
    ("FM_PRIV所属", Write, "チケットPRIV", A),
    ("管理者未所属", Read, "チケットPRIV", N),
    ("Guest_PUB", Write, "チケットPUB", A),
    ("Guest_PUB", Read, "チケットPRIV", N),
    ("PJGuest", Read, "チケットPRIV_PROJ", A),
    ("PJGuest", Create, "チケットPRIV_PROJ", A), // プロジェクト Guest はチケットを作れる(§15-3)
    ("PJGuest", Read, "チケットPRIV", N),        // 同じチームでも、プロジェクト外は見えない
    ("FM未所属", Delete, "チケットPUB", F), // 未所属・作成者でもない → 削除できない(見えるので 403)
    ("FM_PUB所属", Delete, "チケットPUB", A), // 所属メンバー(members)は削除できる
    ("キー_FM_PUB所属", Delete, "チケットPUB_自作(11)", A), // キーでも単体の削除は可(§15-12)
    ("キー_FM_PUB所属", BulkDelete, "チケットPUB_自作(11)", F), // 一括削除はキー不可
    ("Guest_PUB", Delete, "チケットPUB", F), // Guest は作成者でなければ削除できない
    ("FM未所属", Read, "チケット_チーム無し_作成者10", A),
    ("FM_PUB所属", Read, "チケット_チーム無し_作成者10", N),
    ("管理者未所属", Read, "チケット_チーム無し_作成者10", N), // 管理者も例外にしない
    // ── プロジェクト(設計書 §4.5) ──
    ("FM未所属", Read, "プロジェクトPRIV", N),
    ("FM未所属", Read, "プロジェクトPUB+PRIV", A), // 参加チームのどれかが見えれば見える
    ("PJGuest", Read, "プロジェクトPRIV", A),
    ("PJGuest", Write, "プロジェクトPRIV", F), // Guest はプロジェクトを編集できない
    ("FM未所属", Create, "プロジェクトPUB+PRIV", F), // 参加チームのすべてが見えることが必要
    ("FM_PRIV所属", Create, "プロジェクトPUB+PRIV", A),
    ("FM_PRIV所属", Delete, "プロジェクトPRIV", A),
    ("FM未所属", Delete, "プロジェクトPUB+PRIV", F),
    // ── サイクル・Wiki ──
    ("Guest_PUB", Write, "サイクルPUB", A), // チーム Guest は書き込める(§15-4)
    ("PJGuest", Read, "サイクルPRIV", N),   // プロジェクト Guest はサイクル不可
    ("PJGuest", Read, "WikiPRIV_PROJ", N),  // プロジェクト Guest は Wiki 不可
    ("FM_PRIV所属", Write, "WikiPRIV_PROJ", A),
    ("FM未所属", Read, "Wiki全体", A),
    ("Guest_PUB", Read, "Wiki全体", N), // 全体の Wiki は Full Member だけ
    ("FM未所属", Delete, "Wiki全体", F),
    ("管理者未所属", Delete, "Wiki全体", A),
    // ── 共通マスタ ──
    ("Guest_PUB", Read, "マスタ", A),
    ("FM未所属", Write, "マスタ", F),
    ("管理者未所属", Write, "マスタ", A),
    // ── ロードマップ(設計書 §5.2・§4.5) ──
    ("FM未所属", Read, "ロードマップ", A),
    ("FM未所属", Delete, "ロードマップ", A), // 作成者でなくても(owner は責任の表示)
    ("Guest_PUB", Read, "ロードマップ", N),  // Guest は見えない
    ("PJGuest", Write, "ロードマップ", N),
    // ── ユーザー(設計書 §5.2) ──
    ("FM未所属", Read, "ユーザーFM", A),
    ("FM未所属", Read, "ユーザーGuest_PRIV", N), // Private だけにいる Guest は見えない
    ("FM未所属", Read, "ユーザーGuest_PUB", A),
    ("Guest_PUB", Read, "ユーザーFM", N), // Guest は同じチームの人だけ
    ("Guest_PUB", Read, "ユーザーGuest_PUB", A),
    ("管理者未所属", ManageSettings, "ユーザーFM", A),
    ("FM未所属", ManageSettings, "ユーザーFM", F),
];

#[test]
fn policy_matches_the_table() {
    let mut failures = Vec::new();
    for (vn, action, rn, want) in TABLE {
        let got = can(&v(vn), *action, &r(rn));
        if got != *want {
            failures.push(format!(
                "{vn} / {action:?} / {rn}: 期待 {want:?}、実際 {got:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "表と一致しない行:\n{}",
        failures.join("\n")
    );
}

#[test]
fn visible_teams_and_grants() {
    assert_eq!(
        v("FM未所属").visible_team_ids(),
        vec![PUB, PUB_OWNERS, PUB_OWNERS_NONE]
    );
    assert_eq!(
        v("FM_PRIV所属").visible_team_ids(),
        vec![PUB, PRIV, PUB_OWNERS, PUB_OWNERS_NONE]
    );
    assert_eq!(v("Guest_PUB").visible_team_ids(), vec![PUB]);
    assert!(v("PJGuest").visible_team_ids().is_empty());
    assert_eq!(v("PJGuest").project_grants(), vec![(PRIV, PROJ)]);
    assert_eq!(v("連携_PUB").visible_team_ids(), vec![PUB]);
}
