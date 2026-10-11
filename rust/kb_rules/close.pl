% クローズ可否の判定ルール(Scryer Prolog)。
% 事実は Rust 側が必要な分だけ生成して連結する。ここには事実を置かない。
%
% 入力される事実:
%   target(Key).                         判定対象
%   ticket(Key, Kind, Status, Parent).   Kind = bug | feature, Parent = none | 親キー
%   label(Key, Name).
%   strict(Key).                         必須セクションを課す対象(type: ラベル付き)
%   subticket(Key).                      対象がサブチケット(担当者なしを警告しない)
%   section_len(Key, 正規化名, 文字数).
%   assignee_count(Key, N).
%   status_done(Status).
%   required_section(Kind, 名). requires_assignee(Kind). min_len(N).
%   facts_count(N).                      上記の事実(facts_count 自身を除く)の総行数
% 出力: OK|Key  /  NG|Key|コード|引数  の後に END|行数 / 整合性違反は ERR|fact_mismatch

:- use_module(library(lists)).
:- use_module(library(format)).

:- dynamic(target/1).
:- dynamic(ticket/4).
:- dynamic(label/2).
:- dynamic(strict/1).
:- dynamic(subticket/1).
:- dynamic(section_len/3).
:- dynamic(assignee_count/2).
:- dynamic(status_done/1).
:- dynamic(required_section/2).
:- dynamic(requires_assignee/1).
:- dynamic(min_len/1).
:- dynamic(facts_count/1).
% 事実は、種類が混ざって並んでも、前の節を黙って置き換えないよう、discontiguous を宣言する
% (宣言が無いと、後から出た節で前の節が捨てられる。facts_count の件数照合が検出するが、判定が止まる)。
:- discontiguous(target/1).
:- discontiguous(ticket/4).
:- discontiguous(label/2).
:- discontiguous(strict/1).
:- discontiguous(subticket/1).
:- discontiguous(section_len/3).
:- discontiguous(assignee_count/2).
:- discontiguous(status_done/1).
:- discontiguous(required_section/2).
:- discontiguous(requires_assignee/1).
:- discontiguous(min_len/1).
:- discontiguous(facts_count/1).

% 子孫を辿る。循環データでも止まるよう訪問済みを持つ
descendant(T, D) :- descendant_(T, D, [T]).
descendant_(T, D, Seen) :-
    ticket(C, _, _, T),
    \+ memberchk(C, Seen),
    (   D = C
    ;   descendant_(C, D, [C|Seen])
    ).

filled(T, Name) :- section_len(T, Name, L), min_len(M), L >= M.

% クローズを妨げる理由 blocker(チケット, コード, 引数)
blocker(T, open_subtask, C) :-
    descendant(T, C), ticket(C, _, S, _), \+ status_done(S).
blocker(T, missing_section, N) :-
    strict(T), ticket(T, K, _, _), required_section(K, N), \+ filled(T, N).
% 担当者なし(サブチケットには求めない。担当者は親で決めるのが普通で、子ごとに求めると警告が多すぎる)
blocker(T, no_assignee, none) :-
    ticket(T, K, _, _), requires_assignee(K), assignee_count(T, 0), \+ subticket(T).
% 機能追加と障害の混在は禁止(運用ルール)
blocker(T, mixed_type, none) :-
    label(T, 'type:feature'),
    (   label(T, 'type:incident')
    ;   label(T, 'type:infra')
    ;   ticket(T, bug, _, _)
    ).

% --- 整合性: Rust が生成した事実の件数と、実際に読み込めた件数が一致すること ---
% 構文エラーの事実は Scryer が黙って捨てるため、件数照合で取りこぼしを検出する。
count_of(Goal, N) :- findall(x, Goal, L), length(L, N).

loaded_facts(N) :-
    count_of(target(_), N1),
    count_of(ticket(_, _, _, _), N2),
    count_of(label(_, _), N3),
    count_of(strict(_), N4),
    count_of(subticket(_), N4b),
    count_of(section_len(_, _, _), N5),
    count_of(assignee_count(_, _), N6),
    count_of(status_done(_), N7),
    count_of(required_section(_, _), N8),
    count_of(requires_assignee(_), N9),
    count_of(min_len(_), N10),
    N is N1+N2+N3+N4+N4b+N5+N6+N7+N8+N9+N10.

integrity_ok :- facts_count(Expected), loaded_facts(Actual), Expected =:= Actual.

% 出力: 判定行(OK|… または NG|…)を並べ、最後に必ず END|判定行数 を出す。
% END が無い出力は Rust 側で破棄する(途中終了・エラーを「可」と誤認しないため)。
run_close :-
    (   \+ integrity_ok
    ->  format("ERR|fact_mismatch~n", [])
    ;   target(T),
        (   \+ ticket(T, _, _, _)
        ->  format("NG|~a|unknown_ticket|~a~nEND|1~n", [T, T])
        ;   findall(C-A, blocker(T, C, A), Bs0),
            sort(Bs0, Bs),
            (   Bs == []
            ->  format("OK|~a~nEND|1~n", [T])
            ;   length(Bs, N),
                print_blockers(T, Bs),
                format("END|~d~n", [N])
            )
        )
    ).

print_blockers(_, []).
print_blockers(T, [C-A|R]) :-
    format("NG|~a|~a|~a~n", [T, C, A]),
    print_blockers(T, R).
