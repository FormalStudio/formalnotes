import { describe, expect, test } from 'bun:test';

import { blocks, bold, caretAfter, continueList, locate, setKind, tasks, toggleTask } from './editing';

const at = (n: number) => ({ start: n, end: n });

/** Types a return at `caret`. */
const enter = (prev: string, caret: number) => continueList(prev, `${prev.slice(0, caret)}\n${prev.slice(caret)}`, caret);

describe('editing', () => {
  test('tasks are found by line, outside code', () => {
    const body = '- [ ] a\ntext\n  * [x] b\n```\n- [ ] not\n```\n- plain';
    expect(tasks(body)).toEqual([
      { line: 0, done: false, indent: 0, text: 'a' },
      { line: 2, done: true, indent: 2, text: 'b' },
    ]);
  });

  test('a tick flips in place', () => {
    expect(toggleTask('- [ ] a\n- [x] b', 0)).toBe('- [x] a\n- [x] b');
    expect(toggleTask('- [ ] a\n- [x] b', 1)).toBe('- [ ] a\n- [ ] b');
    expect(toggleTask('plain', 0)).toBe('plain');
  });

  test('a return carries the list on', () => {
    expect(enter('- [x] a', 7)).toEqual({ text: '- [x] a\n- [ ] ', selection: at(14) });
    expect(enter('  * one', 7)).toEqual({ text: '  * one\n  * ', selection: at(12) });
    expect(enter('9. nine', 7)).toEqual({ text: '9. nine\n10. ', selection: at(12) });
    expect(enter('- ab', 3)).toEqual({ text: '- a\n- b', selection: at(6) });
  });

  test('a return on an empty item ends the list', () => {
    expect(enter('- [ ] a\n- [ ] ', 14)).toEqual({ text: '- [ ] a\n', selection: at(8) });
    expect(enter('- a\n- \nafter', 6)).toEqual({ text: '- a\n\nafter', selection: at(4) });
  });

  test('other returns are left alone', () => {
    expect(enter('plain', 5)).toBeNull();
    expect(enter('- [ ] a', 3)).toBeNull();
    expect(enter('- a\n\nb', 5)).toBeNull();
    expect(continueList('- a', '- ab', 3)).toBeNull();
  });

  test('lines become tasks and back', () => {
    expect(setKind('milk', at(2), 'task')).toEqual({ text: '- [ ] milk', selection: at(8) });
    expect(setKind('- [x] milk', at(8), 'task')).toEqual({ text: 'milk', selection: at(2) });
    expect(setKind('- milk', at(0), 'task')).toEqual({ text: '- [ ] milk', selection: at(6) });
    expect(setKind('', at(0), 'task')).toEqual({ text: '- [ ] ', selection: at(6) });
  });

  test('a selection of lines changes together and keeps its ticks', () => {
    const body = 'a\n- [x] b\n\nc';
    expect(setKind(body, { start: 0, end: body.length }, 'task')).toEqual({
      text: '- [ ] a\n- [x] b\n\n- [ ] c',
      selection: { start: 6, end: 24 },
    });
    expect(setKind('a\nb', { start: 0, end: 3 }, 'bullet').text).toBe('- a\n- b');
    expect(setKind('- a\n- b', { start: 0, end: 7 }, 'bullet').text).toBe('a\nb');
  });

  test('a heading steps through its levels', () => {
    expect(setKind('- Trip', at(6), 'heading')).toEqual({ text: '## Trip', selection: at(7) });
    expect(setKind('## Trip', at(7), 'heading').text).toBe('### Trip');
    expect(setKind('### Trip', at(8), 'heading')).toEqual({ text: 'Trip', selection: at(4) });
  });

  test('bold wraps, unwraps and opens a pair', () => {
    expect(bold('a word', { start: 2, end: 6 })).toEqual({ text: 'a **word**', selection: { start: 4, end: 8 } });
    expect(bold('a **word**', { start: 4, end: 8 })).toEqual({ text: 'a word', selection: { start: 2, end: 6 } });
    expect(bold('ab', at(1))).toEqual({ text: 'a****b', selection: at(3) });
  });

  test('tasks stand alone as blocks, the lines between them together', () => {
    const body = 'intro\nmore\n- [ ] a\n  - [x] b\n\nend';
    const list = blocks(body);
    expect(list).toEqual([
      { kind: 'text', start: 0, text: 'intro\nmore' },
      { kind: 'task', start: 11, line: 2, marker: '- [ ] ', done: false, text: 'a' },
      { kind: 'task', start: 19, line: 3, marker: '  - [x] ', done: true, text: 'b' },
      { kind: 'text', start: 29, text: '\nend' },
    ]);
    expect(list.map((b) => (b.kind === 'task' ? b.marker + b.text : b.text)).join('\n')).toBe(body);
    expect(blocks('')).toEqual([{ kind: 'text', start: 0, text: '' }]);
  });

  test('a place in the body lands in its block', () => {
    const list = blocks('intro\n- [ ] ab\nend');
    expect(locate(list, 3)).toEqual({ index: 0, at: 3 });
    expect(locate(list, 5)).toEqual({ index: 0, at: 5 });
    expect(locate(list, 6)).toEqual({ index: 1, at: 0 });
    expect(locate(list, 13)).toEqual({ index: 1, at: 1 });
    expect(locate(list, 15)).toEqual({ index: 2, at: 0 });
  });

  test('the caret follows what was typed', () => {
    expect(caretAfter('ab', 'axb')).toBe(2);
    expect(caretAfter('ab', 'abc')).toBe(3);
    expect(caretAfter('abc', 'ac')).toBe(1);
  });
});
