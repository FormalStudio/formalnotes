import { describe, expect, test } from 'bun:test';

import { memorySql } from './bun-sql';
import { parseFolderView, writeFolderView } from './folder-view';
import { editorParts, fileStemFor, joinParts, preview, split, titleOf, withHeading } from './model';
import { formatValue, readProperties, setProperty } from './properties';
import { Library } from './store';

describe('model', () => {
  test('the title is the first heading after the properties, or the file name', () => {
    expect(titleOf('intro\n# Plan\nbody', 'file')).toBe('Plan');
    expect(titleOf('## Sub\ntext', 'file')).toBe('file');
    expect(titleOf('---\n# not: a title\n---\n# Real\n', 'file')).toBe('Real');
  });

  test('a leading heading splits off and composes back, after the properties', () => {
    expect(split('\n# Trip\n\nDay one')).toEqual({ heading: 'Trip', body: 'Day one' });
    expect(withHeading('---\na: b\n---\nplain', 'New')).toBe('---\na: b\n---\n# New\n\nplain');
    const parts = editorParts('---\ndue: 2026-11-14\n---\n# T\n\nBody');
    expect(parts).toEqual({ props: '---\ndue: 2026-11-14\n---\n', heading: 'T', body: 'Body' });
    expect(joinParts(parts.props, parts.heading, parts.body)).toBe('---\ndue: 2026-11-14\n---\n# T\n\nBody');
  });

  test('file names drop what file systems refuse', () => {
    expect(fileStemFor('a/b: c?')).toBe('a b c');
    expect(fileStemFor('..hidden')).toBe('hidden');
    expect(fileStemFor('   ')).toBe('Untitled');
  });

  test('previews skip the title and read blocks', () => {
    expect(preview('# T\n\nIntro **bold**\n- [x] done\n- item\n```\ncode\n```\n> quote', 10)).toEqual([
      { kind: 'text', text: 'Intro bold' },
      { kind: 'task', done: true, text: 'done' },
      { kind: 'bullet', text: 'item' },
      { kind: 'code', text: 'code' },
      { kind: 'quote', text: 'quote' },
    ]);
  });
});

describe('properties', () => {
  const note = '---\nstatus: Doing\ndue: 2026-11-14\ntags: [work, q4]\ndone: false\nnested:\n  a: 1\n# comment\n---\n# Plan the release\n';

  test('reads typed values and skips what it does not model', () => {
    expect(readProperties(note).map((p) => [p.name, p.value])).toEqual([
      ['status', 'Doing'],
      ['due', '2026-11-14'],
      ['tags', ['work', 'q4']],
      ['done', false],
    ]);
  });

  test('writing one property touches only its line', () => {
    const next = setProperty(note, 'status', formatValue('select', 'Done'));
    expect(next).toBe(note.replace('status: Doing', 'status: Done'));
    expect(setProperty(note, 'due', '')).toBe(note.replace('due: 2026-11-14\n', ''));
    expect(setProperty('# T\n', 'done', formatValue('checkbox', true))).toBe('---\ndone: true\n---\n# T\n');
    expect(setProperty('---\ndone: true\n---\n# T\n', 'done', '')).toBe('# T\n');
  });

  test('quotes what would not read back as a string', () => {
    expect(formatValue('text', 'true')).toBe('"true"');
    expect(formatValue('text', '12')).toBe('"12"');
    expect(formatValue('text', ' padded')).toBe('" padded"');
    expect(formatValue('text', 'a: b')).toBe('"a: b"');
    expect(formatValue('text', '#tag')).toBe('"#tag"');
    expect(formatValue('text', 'plain words')).toBe('plain words');
    expect(formatValue('number', -3.5)).toBe('-3.5');
    expect(formatValue('multi_select', ['one', 'a, b', 'x]'])).toBe('[one, "a, b", "x]"]');
    expect(readProperties(`---\nt: ${formatValue('multi_select', ['one', 'a, b'])}\n---\n`)[0].value).toEqual(['one', 'a, b']);
  });
});

describe('folder view', () => {
  test('keeps unknown keys through a rewrite', () => {
    const view = parseFolderView('{"v":1,"view":"table","columns":[{"name":"due","type":"date","x":2}],"theirs":true}');
    expect(view.view).toBe('table');
    const back = JSON.parse(writeFolderView(view));
    expect(back.theirs).toBe(true);
    expect(back.columns[0].x).toBe(2);
    expect(parseFolderView('not json').view).toBe('cards');
  });
});

describe('library store', () => {
  let n = 0;
  const lib = () => new Library(memorySql(), () => `id-${++n}`);

  test('a retitle renames the file and keeps the id', () => {
    const l = lib();
    const id = l.createNote('', '');
    expect(l.entry(id)?.path).toBe('Untitled.md');
    l.retitle(id, 'Weekend plans');
    expect(l.entry(id)?.path).toBe('Weekend plans.md');
    expect(l.note(id)?.text).toBe('# Weekend plans\n');
  });

  test('names are free case-insensitively, and a case-only rename works', () => {
    const l = lib();
    l.createNote('', '# Ideas\n');
    const other = l.createNote('', '# ideas\n');
    expect(l.entry(other)?.path).toBe('ideas 2.md');
    const f = l.createFolder('', 'work');
    l.renameFolder(f, 'Work');
    expect(l.entry(f)?.path).toBe('Work');
  });

  test('trash and restore bring a folder back with its notes', () => {
    const l = lib();
    const f = l.createFolder('', 'Recipes');
    const note = l.createNote('Recipes', '# Soup\n');
    const batch = l.trash(f)!;
    expect(l.all()).toEqual([]);
    expect(l.trashItems().map((t) => t.path)).toEqual(['Recipes']);
    l.restore(batch);
    expect(l.entry(note)?.path).toBe('Recipes/Soup.md');
  });

  test('search matches every word, titles first', () => {
    const l = lib();
    l.createNote('', '# Lentil soup\n\nred lentils');
    l.createNote('', '# Shopping\n\nlentil soup ingredients');
    expect(l.search('lentil soup').map((x) => x.title)).toEqual(['Lentil soup', 'Shopping']);
    expect(l.search('100%')).toEqual([]);
  });
});
