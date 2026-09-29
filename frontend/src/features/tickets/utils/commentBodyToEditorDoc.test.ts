import { createRef } from 'react';
import { generateText } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { describe, expect, it } from 'vitest';
import { commentBodyToEditorDoc, createMentionExtension } from './createMentionExtension';

function extensions() {
  return [
    StarterKit.configure({
      heading: false,
      bulletList: false,
      orderedList: false,
      blockquote: false,
      codeBlock: false,
      horizontalRule: false,
      bold: false,
      italic: false,
      strike: false,
      code: false,
    }),
    createMentionExtension(createRef()),
  ];
}

function roundTrip(body: string) {
  return generateText(commentBodyToEditorDoc(body), extensions(), { blockSeparator: '\n' });
}

describe('commentBodyToEditorDoc', () => {
  it('テキストとメンションを段落の中に置く', () => {
    const doc = commentBodyToEditorDoc('確認 @[日高直樹:12] です');
    expect(doc.content.every((node) => node.type === 'paragraph')).toBe(true);
    const inline = doc.content[0].content ?? [];
    expect(inline.map((node) => node.type)).toEqual(['text', 'mention', 'text']);
    expect(inline[1]).toMatchObject({ type: 'mention', attrs: { id: '12', label: '日高直樹' } });
  });

  it('保存文字列 @[表示名:ID] に往復する', () => {
    const body = '確認 @[日高直樹:12] です\n次の行';
    expect(roundTrip(body)).toBe(body);
  });

  it('メンションの無い本文も残す', () => {
    expect(roundTrip('現在調査中です。')).toBe('現在調査中です。');
  });
});
