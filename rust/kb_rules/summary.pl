% 状況サマリの決定ルール(Scryer Prolog)。
% close.pl を先に連結して使う(blocker/3・filled/2・loaded_facts/1 などを再利用する)。
%
% close.pl の事実に加えて入力される事実:
%   status_category(Cat).   対象のステータス分類: completed | cancelled | started | other
%   doc(Path).              本文・コメントに出てきた設計書パス(出現順)
%   signal(runtime_error).  実行時エラーを示す語を検出した
%
% 出力: PHASE|… / ACTION|… / REMAIN|コード|引数(0件以上) / DOC|パス(0件以上) /
%       CONSISTENCY|コード|引数(0件以上) の後に END|行数(END 以外の行数)。
%       整合性違反は ERR|fact_mismatch のみ。

:- dynamic(status_category/1).
:- dynamic(doc/1).
:- dynamic(signal/1).
% 事実は、種類が混ざって並んでも、前の節を黙って置き換えないよう、discontiguous を宣言する
% (宣言が無いと、後から出た節で前の節が捨てられる。facts_count の件数照合が検出するが、判定が止まる)。
:- discontiguous(status_category/1).
:- discontiguous(doc/1).
:- discontiguous(signal/1).

% --- 整合性: close.pl の事実 + このファイルの事実の総数が facts_count と一致すること ---
summary_loaded(N) :-
    count_of(status_category(_), N1),
    count_of(doc(_), N2),
    count_of(signal(_), N3),
    N is N1 + N2 + N3.

summary_integrity :-
    facts_count(Expected),
    loaded_facts(A1),
    summary_loaded(A2),
    Expected =:= A1 + A2.

% --- 部品 ---
done_status :- status_category(completed).
done_status :- status_category(cancelled).

% 「実施した内容」に当たる節が記入済みか(feature は 実施した内容、bug は 対策)
impl_filled(T) :- ticket(T, feature, _, _), filled(T, '実施した内容').
impl_filled(T) :- ticket(T, bug, _, _), filled(T, '対策').

% 残りの阻害要因が「検証結果・まとめ の欠落」だけか(空なら真)
only_verification_missing(Bs) :-
    \+ (   member(C-A, Bs),
           \+ ( C == missing_section, memberchk(A, ['検証結果', 'まとめ']) )
       ).

% 「完了している」と言える根拠があるか。阻害要因が無いだけでは足りない
% (ラベルなし・サブタスクなしの新規チケットも、阻害要因は無いが何も終わっていない)。
%   - 必須セクションを課されたチケットが、すべて記入済み
%   - サブタスクがあり、そのすべてが完了(阻害要因が空なら全部完了)
evidence(T) :- strict(T).
evidence(T) :- ticket(_, _, _, T).

% --- フェーズ(上から最初に当てはまるもの) ---
phase_of(_, _, done) :- done_status, !.
% 実装が書かれていて残りが検証・まとめだけ、または、阻害要因が無く完了の証拠がある
% (サブチケットがあり全部完了、等)ときは、検証待ち
phase_of(T, Bs, verifying) :- impl_filled(T), only_verification_missing(Bs), !.
phase_of(T, [], verifying) :- evidence(T), !.
phase_of(_, Bs, implementing) :-
    (   status_category(started)
    ;   member(open_subtask-_, Bs)
    ), !.
phase_of(_, _, investigating).

% --- 推奨アクション(上から最初に当てはまるもの) ---
action_of(T, [], Phase, close_ready) :-
    (   Phase == done ; evidence(T) ), !.
action_of(T, _, _, bug_fix) :- ticket(T, bug, _, _), !.
action_of(T, _, _, bug_fix) :- signal(runtime_error), impl_filled(T), !.
action_of(_, _, verifying, verify_only) :- !.
action_of(_, _, _, continue_impl).

% --- ステータス整合(該当するものをすべて) ---
list_consistency(Bs, closed_with_open_subtask, C) :-
    status_category(completed), member(open_subtask-C, Bs).
list_consistency(Bs, closed_missing_section, N) :-
    status_category(completed), member(missing_section-N, Bs).

ticket_consistency(T, ready_but_open, none) :-
    \+ done_status, evidence(T), blocker_free(T).

blocker_free(T) :- \+ blocker(T, _, _).

consistency_of(T, Bs, Cs) :-
    findall(Code-Arg, ( list_consistency(Bs, Code, Arg) ; ticket_consistency(T, Code, Arg) ), Cs0),
    sort(Cs0, Cs).

% --- エントリ ---
run_summary :-
    (   \+ summary_integrity
    ->  format("ERR|fact_mismatch~n", [])
    ;   target(T),
        (   \+ ticket(T, _, _, _)
        ->  format("ERR|unknown_ticket~n", [])
        ;   findall(C-A, blocker(T, C, A), Bs0),
            sort(Bs0, Bs),
            phase_of(T, Bs, Phase),
            action_of(T, Bs, Phase, Action),
            findall(P, doc(P), Docs),
            consistency_of(T, Bs, Cs),
            length(Bs, NB),
            length(Docs, ND),
            length(Cs, NC),
            N is 2 + NB + ND + NC,
            format("PHASE|~a~n", [Phase]),
            format("ACTION|~a~n", [Action]),
            print_remain(Bs),
            print_docs(Docs),
            print_consistency(Cs),
            format("END|~d~n", [N])
        )
    ).

print_remain([]).
print_remain([C-A|R]) :- format("REMAIN|~a|~a~n", [C, A]), print_remain(R).

print_docs([]).
print_docs([P|R]) :- format("DOC|~a~n", [P]), print_docs(R).

print_consistency([]).
print_consistency([C-A|R]) :- format("CONSISTENCY|~a|~a~n", [C, A]), print_consistency(R).
