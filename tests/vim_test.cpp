#include <catch2/catch_test_macros.hpp>

#include <ostream>
#include <string>
#include <string_view>

#include <ftxui/component/event.hpp>

#include "interaction/editor.hpp"
#include "interaction/vim.hpp"

namespace booth::vim {
inline std::ostream& operator<<(std::ostream& os, const Position& position) {
  return os << "(" << position.line << ", " << position.col << ")";
}
}  // namespace booth::vim

namespace {

using ftxui::Event;

struct Harness {
  std::string content;
  booth::Editor editor;

  explicit Harness(std::string text, bool cursor_to_start = true)
      : content(std::move(text)), editor(content) {
    editor.EnableVim(true);
    editor.SetContent(content);
    if (cursor_to_start) {
      press(Event::Escape);
      press(Event::Character('g'));
      press(Event::Character('g'));
    }
  }

  void press(const Event& event) { editor.HandleEvent(event); }

  void keys(std::string_view text) {
    for (std::size_t at = 0; at < text.size();) {
      const std::size_t end = booth::vim::next_grapheme_start(text, at);
      press(Event::Character(text.substr(at, end - at)));
      at = end;
    }
  }

  void insert(std::string_view text) {
    for (std::size_t at = 0; at < text.size();) {
      const std::size_t end = booth::vim::next_grapheme_start(text, at);
      press(Event::Character(text.substr(at, end - at)));
      at = end;
    }
  }

  [[nodiscard]] booth::vim::Position cursor() const {
    return {static_cast<int>(editor.CursorLine()), static_cast<int>(editor.CursorColumn())};
  }
};

}  // namespace

TEST_CASE("vim grapheme helpers keep clusters intact") {
  const std::string text = "a😀b";
  CHECK(booth::vim::count_graphemes(text) == 3);
  CHECK(booth::vim::next_grapheme_start(text, 0) == 1);
  CHECK(booth::vim::next_grapheme_start(text, 1) == 5);
  CHECK(booth::vim::prev_grapheme_start(text, text.size()) == 5);
  CHECK(booth::vim::last_grapheme_start(text) == 5);
  CHECK(booth::vim::step_graphemes(text, 0, 1, true) == 1);
  CHECK(booth::vim::step_graphemes(text, 1, 1, false) == 0);
  CHECK(booth::vim::step_graphemes(text, 0, 2, false) == 0);

  const std::string family = "x👨‍👩‍👧‍👦y";
  CHECK(booth::vim::count_graphemes(family) == 3);
  CHECK(booth::vim::next_grapheme_start(family, 1) == family.size() - 1);

  const std::string flag = "x🇺🇸y";
  CHECK(booth::vim::count_graphemes(flag) == 3);

  const std::string combining = "xe\u0301y";
  CHECK(booth::vim::count_graphemes(combining) == 3);
  CHECK(booth::vim::next_grapheme_start(combining, 1) == 4);
}

TEST_CASE("vim insert-session diff records deletes and typed text") {
  const auto deleted = booth::vim::diff_insert_session("abcd", 4, "abX");
  CHECK(deleted.delete_before == 2);
  CHECK(deleted.delete_after == 0);
  CHECK(deleted.text == "X");

  const auto masked = booth::vim::diff_insert_session("abX", 2, "abbX");
  CHECK(masked.delete_before == 0);
  CHECK(masked.delete_after == 0);
  CHECK(masked.text == "b");

  const auto replaced_emoji = booth::vim::diff_insert_session("😀 abc", 4, "😇 abc");
  CHECK(replaced_emoji.text == "😇");
  CHECK(replaced_emoji.delete_before == 1);
}

TEST_CASE("vim starts in insert and Escape enters normal") {
  std::string text;
  booth::Editor editor(text);
  editor.EnableVim(true);
  CHECK(editor.ModeLabel() == "INSERT");
  editor.HandleEvent(Event::Character('a'));
  editor.HandleEvent(Event::Character('b'));
  CHECK(editor.HandleEvent(Event::Escape));
  CHECK(editor.VimNormal());
  CHECK(editor.ModeLabel() == "NORMAL");
  editor.HandleEvent(Event::Character('z'));
  CHECK(text == "ab");
}

TEST_CASE("vim quiet normal Escape belongs to the host") {
  Harness harness("abc");
  CHECK(harness.editor.VimNormal());
  CHECK_FALSE(harness.editor.VimConsumesEscape());
  CHECK_FALSE(harness.editor.HandleEvent(Event::Escape));
}

TEST_CASE("vim half-typed commands claim Escape") {
  Harness harness("abc");
  harness.keys("2");
  CHECK(harness.editor.VimConsumesEscape());
  CHECK(harness.editor.VimPending() == "2");
  harness.press(Event::Escape);
  CHECK(harness.editor.VimPending().empty());
  CHECK_FALSE(harness.editor.VimConsumesEscape());
}

TEST_CASE("vim insert entry points i a I A o O") {
  Harness harness("ab");
  harness.keys("i");
  CHECK(harness.editor.VimMode() == booth::vim::Mode::Insert);
  harness.insert("X");
  CHECK(harness.content == "Xab");
  harness.press(Event::Escape);
  harness.keys("A");
  harness.insert("Z");
  CHECK(harness.content == "XabZ");

  Harness lines("one");
  lines.keys("o");
  lines.insert("two");
  CHECK(lines.content == "one\ntwo");
  lines.press(Event::Escape);
  lines.keys("O");
  lines.insert("mid");
  CHECK(lines.content == "one\nmid\ntwo");
}

TEST_CASE("vim motions never edit the buffer") {
  Harness harness("alfa\nbeta");
  harness.keys("lljhk");
  CHECK(harness.content == "alfa\nbeta");

  Harness edges("alfa beta");
  edges.keys("$");
  CHECK(edges.cursor() == booth::vim::Position{0, 8});
  edges.keys("0");
  CHECK(edges.cursor() == booth::vim::Position{0, 0});
}

TEST_CASE("vim word motions and counts") {
  Harness harness("alfa beta gamma delta");
  harness.keys("w");
  CHECK(harness.cursor() == booth::vim::Position{0, 5});
  harness.keys("w");
  harness.keys("b");
  CHECK(harness.cursor() == booth::vim::Position{0, 5});
  harness.keys("3w");
  CHECK(harness.cursor() == booth::vim::Position{0, 20});
  harness.keys("e");
  CHECK(harness.editor.VimNormal());
  CHECK(harness.cursor() == booth::vim::Position{0, 20});
  harness.keys("2b");
  CHECK(harness.cursor() == booth::vim::Position{0, 10});
}

TEST_CASE("vim gg and G jump to buffer edges") {
  Harness harness("one\ntwo\nthree");
  harness.keys("G");
  CHECK(harness.cursor() == booth::vim::Position{2, 0});
  harness.keys("A");
  harness.insert("!");
  CHECK(harness.content == "one\ntwo\nthree!");
  harness.press(Event::Escape);
  harness.keys("gg");
  CHECK(harness.cursor() == booth::vim::Position{0, 0});
  harness.keys("2G");
  CHECK(harness.cursor() == booth::vim::Position{1, 0});
}

TEST_CASE("vim desired column survives short lines and re-anchors") {
  const std::string buf = "alfa bravo charlie\nxy\ndelta echo foxtrot";
  Harness harness(buf);
  harness.keys("12l");
  CHECK(harness.cursor() == booth::vim::Position{0, 12});
  harness.keys("j");
  CHECK(harness.cursor() == booth::vim::Position{1, 1});
  harness.keys("j");
  CHECK(harness.cursor() == booth::vim::Position{2, 12});

  Harness reset(buf);
  reset.keys("12l");
  reset.keys("j");
  reset.keys("0");
  reset.keys("j");
  CHECK(reset.cursor() == booth::vim::Position{2, 0});

  Harness counted(buf);
  counted.keys("12l");
  counted.keys("j");
  counted.keys("1j");
  CHECK(counted.cursor() == booth::vim::Position{2, 12});

  Harness sticky(buf);
  sticky.keys("$");
  CHECK(sticky.cursor() == booth::vim::Position{0, 17});
  sticky.keys("j");
  CHECK(sticky.cursor() == booth::vim::Position{1, 1});
  sticky.keys("j");
  CHECK(sticky.cursor() == booth::vim::Position{2, 17});
}

TEST_CASE("vim find and till land correctly and repeat") {
  Harness harness("abcde");
  harness.keys("fc");
  CHECK(harness.cursor() == booth::vim::Position{0, 2});
  harness.keys("Fa");
  CHECK(harness.cursor() == booth::vim::Position{0, 0});
  harness.keys("tc");
  CHECK(harness.cursor() == booth::vim::Position{0, 1});
  harness.keys("$Tc");
  CHECK(harness.cursor() == booth::vim::Position{0, 3});
}

TEST_CASE("vim find counts stay on the line and repeat with semicolon") {
  Harness harness("abaca\nzzz");
  harness.keys("2fa");
  CHECK(harness.cursor() == booth::vim::Position{0, 4});
  harness.keys("fz");
  CHECK(harness.cursor() == booth::vim::Position{0, 4});

  Harness repeat("a-b-c");
  repeat.keys("f-");
  CHECK(repeat.cursor() == booth::vim::Position{0, 1});
  repeat.keys(";");
  CHECK(repeat.cursor() == booth::vim::Position{0, 3});
  repeat.keys(",");
  CHECK(repeat.cursor() == booth::vim::Position{0, 1});

  Harness till("a,b,c,d");
  till.keys("t,");
  CHECK(till.cursor() == booth::vim::Position{0, 0});
  till.keys(";");
  CHECK(till.cursor() == booth::vim::Position{0, 2});
}

TEST_CASE("vim operator find includes the match and till stops short") {
  Harness found("abcde");
  found.keys("dfc");
  CHECK(found.content == "de");

  Harness till("abcde");
  till.keys("dtc");
  CHECK(till.content == "cde");
}

TEST_CASE("vim cancelling a find and finding a digit or emoji") {
  Harness harness("a1b");
  harness.keys("f");
  CHECK(harness.editor.VimConsumesEscape());
  harness.press(Event::Escape);
  CHECK(harness.cursor() == booth::vim::Position{0, 0});
  harness.keys("f1");
  CHECK(harness.cursor() == booth::vim::Position{0, 1});

  const std::string text = "a😀b";
  Harness emoji(text);
  emoji.keys("f😀");
  CHECK(emoji.cursor().col == static_cast<int>(text.find("😀")));
}

TEST_CASE("vim x D and C edit the expected ranges") {
  Harness harness("abcd");
  harness.keys("x");
  CHECK(harness.content == "bcd");
  harness.keys("2x");
  CHECK(harness.content == "d");

  Harness line("alfa beta");
  line.keys("wD");
  CHECK(line.content == "alfa ");
  line.keys("0C");
  CHECK(line.editor.VimMode() == booth::vim::Mode::Insert);
  line.insert("new");
  CHECK(line.content == "new");
}

TEST_CASE("vim D and C honor a count across lines") {
  Harness deleter("alfa beta\ngamma delta\nepsilon");
  deleter.keys("w2D");
  CHECK(deleter.content == "alfa \nepsilon");

  Harness changer("alfa beta\ngamma delta\nepsilon");
  changer.keys("w2C");
  CHECK(changer.editor.VimMode() == booth::vim::Mode::Insert);
  changer.insert("new");
  CHECK(changer.content == "alfa new\nepsilon");
}

TEST_CASE("vim dw dd cw cc and undo") {
  Harness harness("alfa beta\nsecond line");
  harness.keys("dw");
  CHECK(harness.content == "beta\nsecond line");
  harness.keys("dd");
  CHECK(harness.content == "second line");
  harness.keys("u");
  CHECK(harness.content == "beta\nsecond line");

  Harness change("alfa beta");
  change.keys("cw");
  CHECK(change.editor.VimMode() == booth::vim::Mode::Insert);
  change.insert("new");
  CHECK(change.content == "new beta");

  Harness lines("alfa beta\nsecond");
  lines.keys("cc");
  CHECK(lines.editor.VimMode() == booth::vim::Mode::Insert);
  lines.insert("fresh");
  CHECK(lines.content == "fresh\nsecond");
}

TEST_CASE("vim yy then p duplicates a line") {
  Harness harness("alfa\nbeta");
  harness.keys("yyp");
  CHECK(harness.content == "alfa\nalfa\nbeta");
}

TEST_CASE("vim word objects iw aw and counts") {
  Harness inner("alfa beta gamma");
  inner.keys("wdiw");
  CHECK(inner.editor.VimNormal());
  CHECK(inner.content == "alfa  gamma");

  Harness around("alfa beta gamma");
  around.keys("wdaw");
  CHECK(around.content == "alfa gamma");

  Harness tail("alfa beta");
  tail.keys("wdaw");
  CHECK(tail.content == "alfa");

  Harness counted("alfa beta gamma");
  counted.keys("d2aw");
  CHECK(counted.content == "gamma");

  Harness change("alfa beta");
  change.keys("ciw");
  CHECK(change.editor.VimMode() == booth::vim::Mode::Insert);
  change.insert("x");
  CHECK(change.content == "x beta");
}

TEST_CASE("vim quote objects including empty and around") {
  Harness inner("say \"hello there\" now");
  inner.keys("di\"");
  CHECK(inner.content == "say \"\" now");

  Harness around("say \"hello there\" now");
  around.keys("da\"");
  CHECK(around.content == "say now");

  Harness empty("say \"\" now");
  empty.keys("ci\"");
  CHECK(empty.editor.VimMode() == booth::vim::Mode::Insert);
  empty.insert("hi");
  CHECK(empty.content == "say \"hi\" now");
}

TEST_CASE("vim bracket objects span nesting and lines") {
  Harness nested("call(one, nest(two), three)");
  nested.keys("15l");
  nested.keys("di(");
  CHECK(nested.content == "call(one, nest(), three)");

  Harness braces("fn {\n  body\n}\ntail");
  braces.keys("jda{");
  CHECK(braces.content == "fn \ntail");
}

TEST_CASE("vim paragraph objects ip and ap") {
  Harness inner("one\ntwo\n\nthree");
  inner.keys("dip");
  CHECK(inner.content == "\nthree");

  Harness around("one\ntwo\n\nthree");
  around.keys("dap");
  CHECK(around.content == "three");

  Harness end("one\ntwo\n\nthree");
  end.keys("Gdap");
  CHECK(end.content == "one\ntwo");
}

TEST_CASE("vim visual mode edits and renders a selection") {
  Harness harness("alfa beta");
  harness.keys("v");
  CHECK(harness.editor.VimMode() == booth::vim::Mode::Visual);
  CHECK(harness.editor.VimSelectedLines() == 1);
  harness.keys("llll");
  const auto selection = harness.editor.VimSelection();
  REQUIRE(selection.has_value());
  CHECK(selection->from == booth::vim::Position{0, 0});
  CHECK(selection->to == booth::vim::Position{0, 5});
  CHECK_FALSE(selection->linewise);
  harness.keys("d");
  CHECK(harness.content == "beta");
  CHECK(harness.editor.VimNormal());
  CHECK_FALSE(harness.editor.VimSelection().has_value());
}

TEST_CASE("vim visual yank and paste") {
  Harness harness("alfa beta");
  harness.keys("vllly$p");
  CHECK(harness.content == "alfa betaalfa");
}

TEST_CASE("vim visual selections cross lines and V is linewise") {
  Harness chars("alfa\nbeta\ngamma");
  chars.keys("vj");
  const auto spans = chars.editor.VimSelectionSpans();
  REQUIRE(spans.size() == 2);
  CHECK(spans[0].from == booth::vim::Position{0, 0});
  CHECK(spans[0].to == booth::vim::Position{0, 4});
  CHECK(spans[1].from == booth::vim::Position{1, 0});
  CHECK(spans[1].to == booth::vim::Position{1, 1});
  chars.keys("d");
  CHECK(chars.content == "eta\ngamma");

  Harness lines("alfa\nbeta\ngamma");
  lines.keys("lV");
  CHECK(lines.editor.ModeLabel() == "VISUAL LINE");
  lines.keys("jd");
  CHECK(lines.content == "gamma");
}

TEST_CASE("vim Escape leaves visual mode without editing") {
  Harness harness("alfa beta");
  harness.keys("vll");
  CHECK(harness.editor.VimConsumesEscape());
  harness.press(Event::Escape);
  CHECK(harness.editor.VimNormal());
  CHECK(harness.content == "alfa beta");
  CHECK_FALSE(harness.editor.VimSelection().has_value());
}

TEST_CASE("vim viw selects the word so the next operator applies") {
  Harness harness("alfa beta");
  harness.keys("viw");
  CHECK(harness.editor.VimMode() == booth::vim::Mode::Visual);
  harness.keys("d");
  CHECK(harness.content == " beta");
}

TEST_CASE("vim pending text echoes object and find prefixes") {
  Harness harness("alfa beta");
  harness.keys("di");
  CHECK(harness.editor.VimPending() == "di");
  CHECK(harness.editor.ModeLabel() == "NORMAL di");
  CHECK(harness.editor.VimConsumesEscape());
  harness.press(Event::Escape);
  CHECK(harness.editor.VimPending().empty());
  harness.keys("w");
  CHECK(harness.content == "alfa beta");
}

TEST_CASE("vim r replaces the character and refuses to run past the line") {
  Harness harness("abcd");
  harness.keys("lr");
  CHECK(harness.editor.VimConsumesEscape());
  harness.keys("X");
  CHECK(harness.content == "aXcd");
  CHECK(harness.editor.VimNormal());
  CHECK(harness.cursor() == booth::vim::Position{0, 1});

  Harness counted("abcd");
  counted.keys("3rz");
  CHECK(counted.content == "zzzd");
  CHECK(counted.cursor() == booth::vim::Position{0, 2});
  counted.keys("$2rQ");
  CHECK(counted.content == "zzzd");
}

TEST_CASE("vim Replace mode overwrites, restores on Backspace, and appends past EOL") {
  Harness harness("abcd");
  harness.keys("R");
  CHECK(harness.editor.VimMode() == booth::vim::Mode::Replace);
  harness.insert("XY");
  CHECK(harness.content == "XYcd");
  harness.press(Event::Backspace);
  CHECK(harness.content == "Xbcd");
  harness.insert("Z");
  CHECK(harness.content == "XZcd");
  harness.press(Event::Escape);
  CHECK(harness.editor.VimNormal());
  CHECK(harness.cursor() == booth::vim::Position{0, 1});

  Harness end("ab");
  end.keys("$R");
  end.insert("YZ");
  CHECK(end.content == "aYZ");
}

TEST_CASE("vim Replace mode newlines split the line") {
  Harness harness("abcd\nefgh");
  harness.keys("R");
  harness.insert("X");
  harness.press(Event::Character('\n'));
  harness.insert("Y");
  CHECK(harness.content == "X\nYcd\nefgh");
}

TEST_CASE("vim bracket match jumps and deletes through pairs") {
  Harness harness("(a(b)c)");
  harness.keys("%");
  CHECK(harness.cursor() == booth::vim::Position{0, 6});
  harness.keys("%");
  CHECK(harness.cursor() == booth::vim::Position{0, 0});
  harness.keys("ll%");
  CHECK(harness.cursor() == booth::vim::Position{0, 4});

  Harness lines("(\n)");
  lines.keys("%");
  CHECK(lines.cursor() == booth::vim::Position{1, 0});
  lines.keys("%d%");
  CHECK(lines.content.empty());

  Harness unmatched("ab(c");
  unmatched.keys("%");
  CHECK(unmatched.cursor() == booth::vim::Position{0, 0});
}

TEST_CASE("vim percent matches quotes and honors a count as a percentage") {
  Harness doubled("say \"hi\"");
  doubled.keys("f\"%");
  CHECK(doubled.cursor() == booth::vim::Position{0, 7});
  doubled.keys("%");
  CHECK(doubled.cursor() == booth::vim::Position{0, 4});

  Harness escaped("say \\\"hi\\\"");
  escaped.keys("%");
  CHECK(escaped.cursor() == booth::vim::Position{0, 0});

  Harness percent("(a)\nb\n  c\nd");
  percent.keys("50%");
  CHECK(percent.cursor() == booth::vim::Position{1, 0});
  percent.keys("75%");
  CHECK(percent.cursor() == booth::vim::Position{2, 2});
}

TEST_CASE("vim paragraph motions land on boundaries") {
  Harness harness("aaa\n\nbbb\n\nccc");
  harness.keys("}");
  CHECK(harness.cursor() == booth::vim::Position{1, 0});
  harness.keys("}");
  CHECK(harness.cursor() == booth::vim::Position{3, 0});
  harness.keys("{");
  CHECK(harness.cursor() == booth::vim::Position{1, 0});

  Harness deletes("aaa\n\nbbb");
  deletes.keys("d}");
  CHECK(deletes.content == "\nbbb");

  Harness one("hello world");
  one.keys("d}");
  CHECK(one.content.empty());

  Harness two("one\ntwo");
  two.keys("}");
  CHECK(two.cursor() == booth::vim::Position{1, 2});
  two.keys("ggd}");
  CHECK(two.content.empty());
}

TEST_CASE("vim join inserts the right gap") {
  Harness spaced("foo\n  bar");
  spaced.keys("J");
  CHECK(spaced.content == "foo bar");
  CHECK(spaced.cursor() == booth::vim::Position{0, 3});

  Harness sentence("end.\nnext");
  sentence.keys("J");
  CHECK(sentence.content == "end.  next");

  Harness paren("call(\n  )");
  paren.keys("J");
  CHECK(paren.content == "call()");
}

TEST_CASE("vim join keeps trailing space, takes a count, and no-ops on the last line") {
  Harness harness("foo \nbar\nbaz");
  harness.keys("J");
  CHECK(harness.content == "foo bar\nbaz");
  harness.keys("gg3J");
  CHECK(harness.content == "foo bar baz");

  Harness empty("foo\n\nbar");
  empty.keys("J");
  CHECK(empty.content == "foo\nbar");

  Harness blank("foo\n   \nbar");
  blank.keys("J");
  CHECK(blank.content == "foo\nbar");
}

TEST_CASE("vim indent shifts by two spaces and stops at the margin") {
  Harness harness("a\nb");
  harness.keys(">>");
  CHECK(harness.content == "  a\nb");
  CHECK(harness.cursor() == booth::vim::Position{0, 2});
  harness.keys("2>>");
  CHECK(harness.content == "    a\n  b");
  harness.keys("gg<<");
  CHECK(harness.content == "  a\n  b");
  harness.keys("<<");
  CHECK(harness.content == "a\n  b");
  harness.keys("<<");
  CHECK(harness.content == "a\n  b");
}

TEST_CASE("vim indent applies to motions, visual selections, and objects") {
  Harness motion("a\nb\nc");
  motion.keys(">j");
  CHECK(motion.content == "  a\n  b\nc");

  Harness visual("a\nb");
  visual.keys("Vj>");
  CHECK(visual.content == "  a\n  b");
  CHECK(visual.editor.VimNormal());

  Harness object("one\ntwo\n\nthree");
  object.keys(">ip");
  CHECK(object.content == "  one\n  two\n\nthree");

  Harness empty("one\n\ntwo");
  empty.keys(">G");
  CHECK(empty.content == "  one\n\n  two");
}

TEST_CASE("vim dot repeats operator changes and undoes in one step") {
  Harness harness("alfa beta gamma");
  harness.keys("dw.");
  CHECK(harness.content == "gamma");
  harness.keys("u");
  CHECK(harness.content == "beta gamma");
}

TEST_CASE("vim dot replaces the original count and keeps the new one") {
  Harness harness("a b c d e f g h i");
  harness.keys("2dw");
  CHECK(harness.content == "c d e f g h i");
  harness.keys("3.");
  CHECK(harness.content == "f g h i");
  harness.keys(".");
  CHECK(harness.content == "i");
}

TEST_CASE("vim dot replays typed text as one undo step per change") {
  Harness harness("alfa beta gamma");
  harness.keys("cw");
  harness.insert("new");
  harness.press(Event::Escape);
  harness.keys("w.");
  CHECK(harness.content == "new new gamma");
  harness.keys("u");
  CHECK(harness.content == "new beta gamma");
  harness.keys("u");
  CHECK(harness.content == "alfa beta gamma");
}

TEST_CASE("vim dot multiplies an insert's text by its count") {
  Harness harness("x");
  harness.keys("A");
  harness.insert("!");
  harness.press(Event::Escape);
  CHECK(harness.content == "x!");
  harness.keys("3.");
  CHECK(harness.content == "x!!!!");
  harness.keys(".");
  CHECK(harness.content == "x!!!!!!!");
}

TEST_CASE("vim dot repeats o once per copy") {
  Harness harness("a");
  harness.keys("o");
  harness.insert("b");
  harness.press(Event::Escape);
  harness.keys("2.");
  CHECK(harness.content == "a\nb\nb\nb");
  CHECK(harness.cursor() == booth::vim::Position{3, 0});
}

TEST_CASE("vim dot replays Backspace past the insert point") {
  Harness harness("abcd\nefgh");
  harness.keys("A");
  harness.press(Event::Backspace);
  harness.press(Event::Backspace);
  harness.insert("X");
  harness.press(Event::Escape);
  CHECK(harness.content == "abX\nefgh");
  harness.keys("j.");
  CHECK(harness.content == "abX\nefX");
}

TEST_CASE("vim an arrow splits the insert session for undo and redo") {
  Harness harness("one\ntwo");
  harness.keys("I");
  harness.insert("x");
  harness.press(Event::ArrowLeft);
  harness.insert("y");
  harness.press(Event::Escape);
  CHECK(harness.content == "yxone\ntwo");
  harness.keys("j.");
  CHECK(harness.content == "yxone\nytwo");
  harness.keys("u");
  CHECK(harness.content == "yxone\ntwo");
  harness.keys("u");
  CHECK(harness.content == "xone\ntwo");
  harness.keys("u");
  CHECK(harness.content == "one\ntwo");
}

TEST_CASE("vim dot overwrites when repeating Replace mode") {
  Harness harness("abcd\nefgh");
  harness.keys("R");
  harness.insert("xy");
  harness.press(Event::Escape);
  harness.keys("j0.");
  CHECK(harness.content == "xycd\nxygh");
}

TEST_CASE("vim dot repeats a Visual change over the same shape") {
  Harness lines("a\nb\nc\nd\ne");
  lines.keys("Vjd");
  CHECK(lines.content == "c\nd\ne");
  lines.keys(".");
  CHECK(lines.content == "e");

  Harness chars("abcdef");
  chars.keys("vld");
  CHECK(chars.content == "cdef");
  chars.keys(".");
  CHECK(chars.content == "ef");
}

TEST_CASE("vim yanks and undo do not become the dot target") {
  Harness harness("abcdef");
  harness.keys("x");
  CHECK(harness.content == "bcdef");
  harness.keys("ywu");
  CHECK(harness.content == "abcdef");
  harness.keys(".");
  CHECK(harness.content == "bcdef");
}

TEST_CASE("vim dot replays an emoji replacement without splitting it") {
  Harness harness("😀 abc\n😇 def");
  harness.keys("l");
  harness.keys("i");
  harness.press(Event::Backspace);
  harness.insert("😇");
  harness.press(Event::Escape);
  harness.keys("jl.");
  CHECK(harness.content == "😇 abc\n😇 def");
}

TEST_CASE("vim a long insert session is still one undo step") {
  Harness harness("seed");
  harness.keys("i");
  harness.insert(std::string(130, ' '));
  harness.press(Event::Escape);
  harness.keys("u");
  CHECK(harness.content == "seed");
}

TEST_CASE("vim Replace recordings preserve newlines for dot") {
  Harness harness("abcd\nefgh");
  harness.keys("R");
  harness.insert("X");
  harness.press(Event::Character('\n'));
  harness.insert("Y");
  harness.press(Event::Escape);
  CHECK(harness.content == "X\nYcd\nefgh");
  harness.keys("j0.");
  CHECK(harness.content == "X\nYcd\nX\nYgh");
  harness.keys("u");
  CHECK(harness.content == "X\nYcd\nefgh");
}

TEST_CASE("vim a no-op edit does not replace the last change") {
  Harness harness("alfa\nbeta");
  harness.keys("x");
  CHECK(harness.content == "lfa\nbeta");
  harness.keys("GJ");
  CHECK(harness.content == "lfa\nbeta");
  harness.keys("gg.");
  CHECK(harness.content == "fa\nbeta");
}

TEST_CASE("vim dot repeats a find that ended on a period") {
  Harness harness("a.b.c");
  harness.keys("df.");
  CHECK(harness.content == "b.c");
  harness.keys(".");
  CHECK(harness.content == "c");
}

TEST_CASE("vim dot replays a backspace masked by retyping the same grapheme") {
  Harness harness("abX\ncdY");
  harness.keys("ll");
  harness.keys("i");
  harness.press(Event::Backspace);
  harness.insert("bb");
  harness.press(Event::Escape);
  CHECK(harness.content == "abbX\ncdY");
  harness.keys("j.");
  CHECK(harness.content == "abbX\ncbbY");
}

TEST_CASE("vim dot deletes whole graphemes at a multiline visual repeat end") {
  Harness harness("a\nb\nc\n😀x");
  harness.keys("vjdj.");
  CHECK(harness.content == "\nx");
}

TEST_CASE("vim enabling vim does not coalesce pre-vim typing") {
  std::string content;
  booth::Editor editor(content);
  editor.HandleEvent(Event::Character('a'));
  editor.HandleEvent(Event::Character('b'));
  editor.HandleEvent(Event::Character('c'));
  editor.EnableVim(true);
  editor.HandleEvent(Event::Character('d'));
  editor.HandleEvent(Event::Character('e'));
  editor.HandleEvent(Event::Character('f'));
  editor.HandleEvent(Event::Escape);
  CHECK(content == "abcdef");
  editor.HandleEvent(Event::Character('u'));
  CHECK(content == "abc");
}

TEST_CASE("vim visual selection accessors expose spans and line counts") {
  Harness harness("one\ntwo\nthree");
  CHECK(harness.editor.VimSelectedLines() == 0);
  harness.keys("V");
  CHECK(harness.editor.VimSelectedLines() == 1);
  const auto first = harness.editor.VimSelection();
  REQUIRE(first.has_value());
  CHECK(first->linewise);
  harness.keys("j");
  CHECK(harness.editor.VimSelectedLines() == 2);
  CHECK(harness.editor.VimAnchor() == booth::vim::Position{0, 0});
  harness.press(Event::Escape);
  CHECK(harness.editor.VimSelectedLines() == 0);
  CHECK(harness.editor.ModeLabel() == "NORMAL");
}

TEST_CASE("vim mode label reports Replace and pending prefixes") {
  Harness harness("alfa bravo charlie");
  CHECK(harness.editor.ModeLabel() == "NORMAL");
  harness.keys("2");
  CHECK(harness.editor.ModeLabel() == "NORMAL 2");
  harness.keys("d");
  CHECK(harness.editor.ModeLabel() == "NORMAL 2d");
  harness.press(Event::Escape);
  CHECK(harness.editor.ModeLabel() == "NORMAL");
  harness.keys("r");
  CHECK(harness.editor.ModeLabel() == "NORMAL r");
  harness.press(Event::Escape);

  Harness replace("abc");
  replace.keys("R");
  CHECK(replace.editor.ModeLabel() == "REPLACE");
}

TEST_CASE("vim backspace moves left in normal mode rather than deleting") {
  Harness harness("alfa", false);
  harness.press(Event::Escape);
  harness.press(Event::Backspace);
  CHECK(harness.content == "alfa");
}

TEST_CASE("vim arrow keys act as motions in normal mode") {
  Harness harness("alfa\nbeta");
  harness.press(Event::ArrowDown);
  CHECK(harness.cursor() == booth::vim::Position{1, 0});
  harness.press(Event::ArrowRight);
  CHECK(harness.cursor() == booth::vim::Position{1, 1});
  harness.press(Event::ArrowUp);
  CHECK(harness.cursor() == booth::vim::Position{0, 1});
  harness.press(Event::ArrowLeft);
  CHECK(harness.cursor() == booth::vim::Position{0, 0});
}

TEST_CASE("vim a pending replace character is cancelled by an arrow") {
  Harness harness("abc");
  harness.keys("r");
  harness.press(Event::ArrowLeft);
  CHECK(harness.content == "abc");
  CHECK(harness.editor.VimPending().empty());
}

TEST_CASE("vim space is a find target and a rightward motion") {
  Harness found("a b");
  found.keys("f ");
  CHECK(found.cursor() == booth::vim::Position{0, 1});

  Harness motion("abc");
  motion.keys(" ");
  CHECK(motion.cursor() == booth::vim::Position{0, 1});
}

TEST_CASE("vim SetCursor places a grapheme-safe caret") {
  std::string content;
  booth::Editor editor(content);
  editor.HandleEvent(Event::Character('a'));
  editor.HandleEvent(Event::Character('b'));
  editor.HandleEvent(Event::Character('c'));
  editor.SetCursor(1);
  CHECK(editor.CursorOffset() == 1);
  editor.HandleEvent(Event::Character('X'));
  CHECK(content == "aXbc");

  std::string emoji;
  booth::Editor cluster(emoji);
  cluster.HandleEvent(Event::Character("a"));
  cluster.HandleEvent(Event::Character("😀"));
  cluster.HandleEvent(Event::Character("b"));
  cluster.SetCursor(3);
  CHECK(cluster.CursorOffset() == 1);
  cluster.HandleEvent(Event::Character('X'));
  CHECK(emoji == "aX😀b");
}

TEST_CASE("vim SetCursor splits an open insert session") {
  Harness harness("abc");
  harness.keys("i");
  harness.editor.SetCursor(2);
  CHECK(harness.editor.CursorOffset() == 2);
  harness.insert("X");
  CHECK(harness.content == "abXc");
  harness.press(Event::Escape);
  harness.keys("u");
  CHECK(harness.content == "abc");
}

TEST_CASE("vim SetCursor clamps a normal-mode caret onto the last grapheme") {
  Harness harness("ab");
  harness.editor.SetCursor(2);
  CHECK(harness.cursor() == booth::vim::Position{0, 1});
}
