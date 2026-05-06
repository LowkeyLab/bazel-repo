# Pinyin Composer

Browser-local pinyin-first Chinese composer for creating inline Hanzi drafts with per-character pinyin annotations.

## Commands

```bash
aspect build //angular/projects/pinyin-composer:pinyin-composer
aspect test //angular/projects/pinyin-composer:test
ibazel run //angular/projects/pinyin-composer:pinyin-composer.serve
```

## User Workflow

1. Type tone-free pinyin into the inline document editor.
2. Choose an inline Hanzi candidate.
3. Review per-Hanzi pinyin annotations when the candidate aligns to individual characters.
4. Continue editing the document text directly.
5. Save the draft locally in this browser.

Drafts are stored on this device only. The v1 app has no account system and no backend sync.
