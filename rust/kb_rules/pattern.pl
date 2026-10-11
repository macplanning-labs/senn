% 成功事例の知見(2B)の決定ルール(Scryer Prolog)。単独で連結して使う(close.pl は不要)。
%
% 入力される事実(Rust が、同じプロジェクトの完了済み・障害でないチケットから作る):
%   sim_ticket(Key, Rank).      候補とその順位(SQL の一致語数の多い順。1 が最良)
%   sim_section(Key, Name).     候補の節の名前(正規化済み)。1 候補につき同じ名前は 1 回
%   sim_label(Key, Label).      候補のラベル
%   sim_file(Key, Path).        候補の本文・コメントに出てきたファイルパス。1 候補につき同じパスは 1 回
%   pitfall(Key, BugKey).       Key のあとに、Key の番号を本文・コメントに書いて起きた不具合
%   facts_count(N).             上の事実の総行数(facts_count 自身を除く)
%
% 出力: REF|キー|品質 / SECTION|名|件数 / LABEL|名|件数 / FILE|パス|件数 /
%       PITFALL|機能キー|不具合キー の後に END|行数(END 以外の行数)。
%       整合性違反は ERR|fact_mismatch のみ。

:- use_module(library(lists)).
:- use_module(library(format)).

% 事実は「候補ごと」に、種類が混ざって並ぶ(sim_ticket → sim_section → sim_label …)。
% 同じ述語の節が飛び飛びになっても、前の節を置き換えないよう、discontiguous を宣言する
% (宣言が無いと、後から出た節で前の節が黙って捨てられる。件数照合が無ければ気づけない)。
:- dynamic(sim_ticket/2).
:- dynamic(sim_section/2).
:- dynamic(sim_label/2).
:- dynamic(sim_file/2).
:- dynamic(pitfall/2).
:- discontiguous(sim_ticket/2).
:- discontiguous(sim_section/2).
:- discontiguous(sim_label/2).
:- discontiguous(sim_file/2).
:- discontiguous(pitfall/2).
:- dynamic(facts_count/1).
:- dynamic(top/2).

% 参考にする事例の最大件数
top_limit(5).

count_of(Goal, N) :- findall(x, Goal, L), length(L, N).

% --- 整合性: Rust が作った事実の総数と、読み込めた件数が一致すること ---
% (構文エラーの事実は Scryer が黙って捨てるため、件数照合で取りこぼしを検出する)
loaded_facts(N) :-
    count_of(sim_ticket(_, _), N1),
    count_of(sim_section(_, _), N2),
    count_of(sim_label(_, _), N3),
    count_of(sim_file(_, _), N4),
    count_of(pitfall(_, _), N5),
    N is N1 + N2 + N3 + N4 + N5.

integrity_ok :- facts_count(Expected), loaded_facts(Actual), Expected =:= Actual.

take(0, _, []) :- !.
take(_, [], []) :- !.
take(N, [X|Xs], [X|Ys]) :- N1 is N - 1, take(N1, Xs, Ys).

% --- 品質 = 100 - 8*順位 - 10*落とし穴の数(後続の不具合が少ないほど高い) ---
pitfall_count(K, N) :-
    findall(B, pitfall(K, B), Bs0), sort(Bs0, Bs), length(Bs, N).

% 品質の高い順(同点はキーの順)に、上位 Limit 件を top/2 に保持する
rank_top :-
    retractall(top(_, _)),
    top_limit(Limit),
    findall(NQ-K, ( sim_ticket(K, R), pitfall_count(K, PC), NQ is 8*R + 10*PC - 100 ), L0),
    sort(L0, L1),
    take(Limit, L1, T),
    assert_top(T).

assert_top([]).
assert_top([NQ-K|T]) :- Q is 0 - NQ, assertz(top(K, Q)), assert_top(T).

top_count(N) :- count_of(top(_, _), N).

% --- 共通パターン: 採用した事例の過半数(かつ 2 件以上)に共通して現れる値 ---
% Pred は sim_section / sim_label / sim_file(いずれも (Key, 値) の 2 引数)
common(Pred, V, C) :-
    findall(X, ( top(K, _), call(Pred, K, X) ), Xs),
    sort(Xs, Us),
    member(V, Us),
    findall(a, member(V, Xs), As), length(As, C),
    top_count(N), C >= 2, C * 2 >= N.

% 件数の多い順(同数は名前の順)に並べて、 値-件数 の組にする
ordered_common(Pred, Pairs) :-
    findall(NC-V, ( common(Pred, V, C), NC is 0 - C ), L0),
    sort(L0, L1),
    neg_pairs(L1, Pairs).

neg_pairs([], []).
neg_pairs([NC-V|T], [V-C|R]) :- C is 0 - NC, neg_pairs(T, R).

% --- エントリ ---
run_pattern :-
    (   \+ integrity_ok
    ->  format("ERR|fact_mismatch~n", [])
    ;   rank_top,
        findall(NQ-K, ( top(K, Q), NQ is 0 - Q ), Rs0),
        sort(Rs0, Rs),
        ordered_common(sim_section, Secs),
        ordered_common(sim_label, Labs),
        ordered_common(sim_file, Files),
        findall(K-B, ( top(K, _), pitfall(K, B) ), Ps0),
        sort(Ps0, Ps),
        length(Rs, N1), length(Secs, N2), length(Labs, N3), length(Files, N4), length(Ps, N5),
        N is N1 + N2 + N3 + N4 + N5,
        print_refs(Rs),
        print_counted('SECTION', Secs),
        print_counted('LABEL', Labs),
        print_counted('FILE', Files),
        print_pitfalls(Ps),
        format("END|~d~n", [N])
    ).

print_refs([]).
print_refs([NQ-K|R]) :- Q is 0 - NQ, format("REF|~a|~d~n", [K, Q]), print_refs(R).

print_counted(_, []).
print_counted(Tag, [V-C|R]) :- format("~a|~a|~d~n", [Tag, V, C]), print_counted(Tag, R).

print_pitfalls([]).
print_pitfalls([K-B|R]) :- format("PITFALL|~a|~a~n", [K, B]), print_pitfalls(R).
