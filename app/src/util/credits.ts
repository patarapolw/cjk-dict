/**
 * Credits for the About page: where the default dictionaries and the other data the app
 * uses come from, and their licenses as their sources state them.
 *
 * Keep in step with `dict-admin/src/defaults.rs` and docs/dict-admin.md's table. Give a
 * license as the source itself states it, linked to the page that says so.
 */

export interface ILink {
  name: string;
  url: string;
}

export interface ICredit {
  /** where the data comes from, e.g. KANJIDIC2 (EDRDG) */
  source: ILink;
  /** the project that converts it to Yomitan, if not dict-admin */
  conversion?: ILink;
  license: ILink;
  /** the credit the source asks for, word for word */
  credit?: string;
}

const CC_BY_SA_4 = {
  name: "CC BY-SA 4.0",
  url: "https://creativecommons.org/licenses/by-sa/4.0/",
};

const CC_BY_SA_4_GFDL = {
  name: "CC BY-SA 4.0 and GFDL",
  url: "https://kaikki.org/dictionary/",
};

const WIKIPEDIA: ICredit = {
  source: {
    name: "Wikipedia, via DBpedia's short abstracts",
    url: "https://dbpedia.org/",
  },
  conversion: {
    name: "MarvNC/wikipedia-yomitan",
    url: "https://github.com/MarvNC/wikipedia-yomitan",
  },
  // DBpedia's, for its releases; Wikipedia's own text is CC BY-SA 4.0 and GFDL now
  license: {
    name: "CC BY-SA 3.0 and GFDL",
    url: "https://www.dbpedia.org/about/",
  },
};

const WIKTIONARY: ICredit = {
  source: {
    name: "Wiktionary, via kaikki.org (wiktextract)",
    url: "https://kaikki.org/",
  },
  conversion: {
    name: "yomidevs/wiktionary-to-yomitan",
    url: "https://github.com/yomidevs/wiktionary-to-yomitan",
  },
  // "the same licenses as Wiktionary - both CC-BY-SA and GFDL"
  license: CC_BY_SA_4_GFDL,
};

/** The default dictionaries, by name in the library */
export const DEFAULT_CREDITS: Record<string, ICredit> = {
  KANJIDIC: {
    source: {
      name: "KANJIDIC2, Electronic Dictionary Research and Development Group",
      url: "https://www.edrdg.org/wiki/index.php/KANJIDIC_Project",
    },
    conversion: {
      name: "scriptin/jmdict-simplified (JSON), converted by dict-admin",
      url: "https://github.com/scriptin/jmdict-simplified",
    },
    license: {
      name: "EDRDG licence (CC BY-SA 4.0)",
      url: "https://www.edrdg.org/edrdg/licence.html",
    },
    credit:
      "This publication has included material from the KANJIDIC2 dictionary file, the property of the Electronic Dictionary Research and Development Group, used in conformance with the Group's licence.",
  },
  Kanjium: {
    source: {
      name: "Kanjium, by Uros O.",
      url: "https://github.com/mifunetoshiro/kanjium",
    },
    license: CC_BY_SA_4,
    credit:
      "The pitch accent notation, verb particle data, phonetics, homonyms and other additions or modifications to EDICT, KANJIDIC or KRADFILE were provided by Uros O. through his free database.",
  },
  Pixiv: {
    source: { name: "pixiv百科事典", url: "https://dic.pixiv.net/" },
    conversion: {
      name: "MarvNC/pixiv-yomitan",
      url: "https://github.com/MarvNC/pixiv-yomitan",
    },
    // not an open license: article 21-2 of pixiv's terms lets users reuse its text
    license: {
      name: "pixiv's terms of use (not an open license)",
      url: "https://policies.pixiv.net/",
    },
  },
  "ja.Wikipedia": WIKIPEDIA,
  "zh.Wikipedia": WIKIPEDIA,
  "ko.Wikipedia": WIKIPEDIA,
  "CC-CEDICT": {
    source: { name: "CC-CEDICT", url: "https://cc-cedict.org/wiki/" },
    conversion: {
      name: "MarvNC/cc-cedict-yomitan",
      url: "https://github.com/MarvNC/cc-cedict-yomitan",
    },
    // the source's; cc-cedict-yomitan's README says 3.0
    license: CC_BY_SA_4,
  },
  "KO-EN.KRDICT": {
    source: {
      name: "Basic Korean Dictionary, National Institute of Korean Language",
      url: "https://krdict.korean.go.kr/eng/mainAction",
    },
    conversion: {
      name: "Lyroxide/yomitan-ko-dic",
      url: "https://github.com/Lyroxide/yomitan-ko-dic",
    },
    // its copyright terms give no version
    license: {
      name: "CC BY-SA",
      url: "https://krdict.korean.go.kr/eng/kboardPolicy/copyRightTermsInfo",
    },
  },
  "wty-ja-en": WIKTIONARY,
  "wty-ko-en": WIKTIONARY,
  "wty-zh-en": WIKTIONARY,
};

/** The other data the app downloads or search.db holds */
export const OTHER_CREDITS: (ICredit & { what: string })[] = [
  {
    what: "Japanese tokenizer dictionary (lindera)",
    source: {
      name: "UniDic 2.1.2, The UniDic Consortium",
      url: "https://github.com/lindera/lindera/blob/main/lindera-unidic/NOTICE.txt",
    },
    license: {
      name: "BSD-style (see its notice)",
      url: "https://github.com/lindera/lindera/blob/main/lindera-unidic/NOTICE.txt",
    },
  },
  {
    what: "Korean tokenizer dictionary (lindera)",
    source: {
      name: "mecab-ko-dic",
      url: "https://github.com/lindera/lindera/blob/main/lindera-ko-dic/NOTICE.txt",
    },
    license: {
      name: "Apache License 2.0",
      url: "https://www.apache.org/licenses/LICENSE-2.0",
    },
  },
  {
    what: "Chinese tokenizer dictionary (lindera)",
    source: {
      name: "CC-CEDICT-MeCab",
      url: "https://github.com/lindera/lindera/blob/main/lindera-cc-cedict/NOTICE.txt",
    },
    license: CC_BY_SA_4,
  },
  {
    what: "Font for rare Han characters",
    source: {
      name: "HanaMinAFDKO (Hanazono Mincho)",
      url: "https://github.com/cjkvi/HanaMinAFDKO",
    },
    license: {
      name: "GlyphWiki license",
      url: "http://glyphwiki.org/license.html",
    },
  },
  {
    what: "How characters break down",
    source: {
      name: "CJKVI IDS database's ids.txt, derived from the CHISE project",
      url: "https://github.com/cjkvi/cjkvi-ids",
    },
    license: {
      name: "the CHISE project's terms",
      url: "http://www.chise.org/",
    },
  },
  {
    what: "Hangul readings of Hanja words",
    source: {
      name: "hanja_hangul's tables, from the National Institute of Korean Language's Standard Korean Dictionary and Urimalsaem",
      url: "https://github.com/p14jeffwest/hanja_hangul",
    },
    license: {
      name: "CC BY-SA 2.0 KR",
      url: "https://creativecommons.org/licenses/by-sa/2.0/kr/",
    },
  },
  {
    what: "Hangul readings of Hanja",
    source: {
      name: "libhangul's hanja.txt",
      url: "https://github.com/libhangul/libhangul",
    },
    license: {
      name: "BSD-3-Clause",
      url: "https://opensource.org/license/bsd-3-clause",
    },
  },
];
