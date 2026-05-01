# Pinyin Composer

Browser-local pinyin-first Chinese composer for creating phrase-level Hanzi + pinyin ruby output.

## Commands

```bash
aspect build //angular/projects/pinyin-composer:pinyin-composer
aspect test //angular/projects/pinyin-composer:test
ibazel run //angular/projects/pinyin-composer:pinyin-composer.serve
```

## User Workflow

1. Type tone-free pinyin into the composer.
2. Choose an inline Hanzi phrase candidate.
3. Review the phrase-level pinyin ruby preview.
4. Click a phrase to reopen candidate correction.
5. Save the draft locally in this browser.
6. Copy the semantic HTML ruby export.

Drafts are stored on this device only. The v1 app has no account system and no backend sync.
