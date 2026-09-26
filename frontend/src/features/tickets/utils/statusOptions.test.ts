import { describe, expect, it } from 'vitest';
import { buildStatusOptions, DEFAULT_STATUS_OPTIONS, statusLabelOf } from './statusOptions';

describe('statusOptions', () => {
  it('ワークフロー設定が無いときは標準の6種類（英語名）', () => {
    const options = buildStatusOptions([]);
    expect(options.map((o) => o.label)).toEqual(['Backlog', 'Todo', 'In Progress', 'Resolved', 'Closed', 'Cancelled']);
    expect(buildStatusOptions(undefined)).toEqual([...DEFAULT_STATUS_OPTIONS]);
  });

  it('ワークフロー設定があれば、その並びと名前（変更・追加したステータスも）', () => {
    const options = buildStatusOptions([
      { slug: 'open', name: 'Todo', color: '#aaa' },
      { slug: 'review', name: 'In Review', color: '#bbb' },
      { slug: 'closed', name: 'Done', color: '#ccc' },
    ]);
    expect(options).toEqual([
      { value: 'open', label: 'Todo', color: '#aaa' },
      { value: 'review', label: 'In Review', color: '#bbb' },
      { value: 'closed', label: 'Done', color: '#ccc' },
    ]);
  });

  it('内部値から表示名: "open" は "Todo"。選択肢に無い値は標準名、それも無ければ内部値', () => {
    const options = buildStatusOptions([{ slug: 'open', name: 'Todo', color: '' }]);
    expect(statusLabelOf('open', options)).toBe('Todo');
    expect(statusLabelOf('in_progress', options)).toBe('In Progress');
    expect(statusLabelOf('custom_x', options)).toBe('custom_x');
    expect(statusLabelOf(null, options)).toBe('—');
  });
});
