/**
 * insights/i18n.ts - Insights 画面の翻訳リソース
 *
 * 文言はすべてここに置き、画面からは t('insights.…') で参照する。
 * 件数などの値は {{n}} で埋め込む(count は複数形の判定に使われるため使わない)。
 */

import i18n from 'i18next';

// 日本語
const jaTranslations = {
  insights: {
    nav: 'Insights',
    title: 'Insights',
    description: '人と AI の作業の速さと、AI の使用量を、数字とグラフで見ます。',
    none: 'データなし',
    loading: '読み込み中…',
    filter: {
      period: '期間',
      granularity: '区切り',
      ticketType: 'チケットの種類',
      project: 'プロジェクト',
      allProjects: 'すべてのプロジェクト',
    },
    period: {
      days: '直近 {{n}} 日',
    },
    granularity: {
      week: '週ごと',
      month: '月ごと',
    },
    ticketType: {
      all: 'すべて',
      bug: 'バグ',
      task: 'タスク',
      feature: '機能',
    },
    tab: {
      cycle: 'サイクルタイム',
      triage: 'トリアージ時間',
      throughput: '処理総量',
      aiUsage: 'AI 使用量',
    },
    segment: {
      human: '人',
      ai: 'AI',
      unassigned: '未割当',
    },
    cycle: {
      help: 'サイクルタイム: 着手から完了までにかかった時間です。',
      median: '中央値',
      p90: '90% パーセンタイル(9 割がこの時間以内)',
      mean: '平均',
      count: '{{n}} 件',
      ratio: 'AI は人の中央値の {{percent}}% の時間です。',
    },
    triage: {
      help: 'トリアージ時間: チケットが依頼されてから、着手されるまでの時間です。',
      chartTitle: '期間ごとの中央値',
      waiting: {
        title: 'まだ着手されず待っているチケット',
        count: '件数',
        median: '待ち時間の中央値',
        max: '最長の待ち時間',
      },
    },
    throughput: {
      help: '完了したチケットと、そのポイントの数です。',
      tickets: '完了件数',
      points: '完了ポイント',
      aiShareTickets: 'AI が担当した割合(件数)',
      aiSharePoints: 'AI が担当した割合(ポイント)',
    },
    aiUsage: {
      help: 'AI 使用量: 加重トークンは、入力・出力・キャッシュのトークンを重みづけして合計した量です。',
      runs: '実行回数',
      tickets: 'チケット数',
      weighted: '加重トークン',
      perRun: '1 実行あたり',
      perTicket: '1 チケットあたり',
      usd: '換算額(API 料金の換算)',
      tokens: 'トークンの内訳',
      input: '入力',
      output: '出力',
      cacheCreation: 'キャッシュ作成',
      cacheRead: 'キャッシュ読み取り',
      seriesTitle: '期間ごとの加重トークン',
    },
    usd: {
      note: 'API で払った場合の換算で、請求額ではありません。',
    },
    days: '{{n}} 日',
    note: {
      noStart: '着手日時のないチケット {{n}} 件は、数字に含めていません。',
      aiUnfinished: '実行中・未完了の AI 実行 {{n}} 件は、数字に含めていません。',
      aiError: 'エラーで終わった AI 実行 {{n}} 件は、数字に含めていません。',
      usageUnfinished: '未完了の AI 実行 {{n}} 件は、使用量に含めていません。',
      noTokens: 'トークン情報のない AI 実行 {{n}} 件は、使用量に含めていません。',
    },
    duration: {
      daysHours: '{{days}} 日 {{hours}} 時間',
      days: '{{days}} 日',
      hoursMinutes: '{{hours}} 時間 {{minutes}} 分',
      hours: '{{hours}} 時間',
      minutes: '{{minutes}} 分',
      lessThanMinute: '1 分未満',
    },
    empty: {
      title: 'まだ数字がありません',
      body: 'AI に渡すと、ここに数字が出ます。',
    },
    error: {
      generic: 'データを取得できませんでした。',
      retry: 'もう一度',
    },
  },
};

// English
const enTranslations = {
  insights: {
    nav: 'Insights',
    title: 'Insights',
    description: 'See how fast people and AI finish work, and how much AI is used, in numbers and charts.',
    none: 'No data',
    loading: 'Loading…',
    filter: {
      period: 'Period',
      granularity: 'Grouping',
      ticketType: 'Ticket type',
      project: 'Project',
      allProjects: 'All projects',
    },
    period: {
      days: 'Last {{n}} days',
    },
    granularity: {
      week: 'Weekly',
      month: 'Monthly',
    },
    ticketType: {
      all: 'All',
      bug: 'Bug',
      task: 'Task',
      feature: 'Feature',
    },
    tab: {
      cycle: 'Cycle time',
      triage: 'Triage time',
      throughput: 'Throughput',
      aiUsage: 'AI usage',
    },
    segment: {
      human: 'Human',
      ai: 'AI',
      unassigned: 'Unassigned',
    },
    cycle: {
      help: 'Cycle time: how long it took from start to completion.',
      median: 'Median',
      p90: '90th percentile (9 in 10 finish within this time)',
      mean: 'Mean',
      count: '{{n}} tickets',
      ratio: 'AI takes {{percent}}% of the human median time.',
    },
    triage: {
      help: 'Triage time: how long a ticket waited from being requested until work started.',
      chartTitle: 'Median per period',
      waiting: {
        title: 'Waiting, not started yet',
        count: 'Count',
        median: 'Median wait',
        max: 'Longest wait',
      },
    },
    throughput: {
      help: 'Completed tickets and their points.',
      tickets: 'Completed tickets',
      points: 'Completed points',
      aiShareTickets: 'AI share (tickets)',
      aiSharePoints: 'AI share (points)',
    },
    aiUsage: {
      help: 'AI usage: weighted tokens are the input, output and cache tokens added together with weights.',
      runs: 'Runs',
      tickets: 'Tickets',
      weighted: 'Weighted tokens',
      perRun: 'Per run',
      perTicket: 'Per ticket',
      usd: 'Estimated cost (API price equivalent)',
      tokens: 'Token breakdown',
      input: 'Input',
      output: 'Output',
      cacheCreation: 'Cache write',
      cacheRead: 'Cache read',
      seriesTitle: 'Weighted tokens per period',
    },
    usd: {
      note: 'This is an API-price equivalent estimate, not the amount billed.',
    },
    days: '{{n}} days',
    note: {
      noStart: '{{n}} tickets without a start time are not included in the numbers.',
      aiUnfinished: '{{n}} unfinished AI runs are not included in the numbers.',
      aiError: '{{n}} AI runs that ended in an error are not included in the numbers.',
      usageUnfinished: '{{n}} unfinished AI runs are not included in usage.',
      noTokens: '{{n}} AI runs without token data are not included in usage.',
    },
    duration: {
      daysHours: '{{days}}d {{hours}}h',
      days: '{{days}}d',
      hoursMinutes: '{{hours}}h {{minutes}}m',
      hours: '{{hours}}h',
      minutes: '{{minutes}}m',
      lessThanMinute: 'Under 1m',
    },
    empty: {
      title: 'No numbers yet',
      body: 'Once tickets are handed to AI, the numbers appear here.',
    },
    error: {
      generic: 'Could not load the data.',
      retry: 'Try again',
    },
  },
};

// リソースバンドルを登録
i18n.addResourceBundle('ja', 'translation', jaTranslations, true, true);
i18n.addResourceBundle('en', 'translation', enTranslations, true, true);
