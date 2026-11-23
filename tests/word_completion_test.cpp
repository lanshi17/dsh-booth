#include <catch2/catch_test_macros.hpp>

#include <string>
#include <vector>

#include <ftxui/component/event.hpp>

#include "interaction/editor.hpp"
#include "interaction/word_completion.hpp"

using booth::build_word_vocabulary;
using booth::word_completion_suffix;

TEST_CASE("word completion ranks history before transcript") {
  const auto vocabulary = build_word_vocabulary({"alpha"}, {"alphabet"});

  REQUIRE(word_completion_suffix("al", 2, vocabulary) == "pha");
}

TEST_CASE("word completion prefers longer then lexically earlier words") {
  const auto longer = build_word_vocabulary({"alpha", "alphabet"}, {});
  REQUIRE(word_completion_suffix("al", 2, longer) == "phabet");

  const auto lexical = build_word_vocabulary({"amber", "ample"}, {});
  REQUIRE(word_completion_suffix("am", 2, lexical) == "ber");
}

TEST_CASE("word completion matches without case and preserves stored spelling") {
  const auto vocabulary = build_word_vocabulary({"Console"}, {});

  REQUIRE(word_completion_suffix("cOn", 3, vocabulary) == "sole");
}

TEST_CASE("word completion inserts at the cursor without replacing following text") {
  std::string prompt = "al-tail";
  booth::Editor editor(prompt);
  editor.SetCursor(2);
  const auto vocabulary = build_word_vocabulary({"alphabet"}, {});
  const auto suffix = word_completion_suffix(prompt, editor.CursorOffset(), vocabulary);

  REQUIRE(suffix == "phabet");
  REQUIRE(editor.HandleEvent(ftxui::Event::Character(*suffix)));
  REQUIRE(prompt == "alphabet-tail");
  REQUIRE(editor.CursorOffset() == 8);
}

TEST_CASE("word completion ignores short words and no matches") {
  const auto vocabulary = build_word_vocabulary({"a xylophone"}, {"beta"});

  REQUIRE_FALSE(word_completion_suffix("x", 1, vocabulary));
  REQUIRE_FALSE(word_completion_suffix("zz", 2, vocabulary));
  REQUIRE_FALSE(word_completion_suffix("beta", 4, vocabulary));
}

TEST_CASE("word completion tokenizes only the supported ASCII characters") {
  const auto vocabulary = build_word_vocabulary({"src/lib-file:2.0_name!"}, {});

  REQUIRE(word_completion_suffix("src/li", 6, vocabulary) == "b-file:2.0_name");
}

TEST_CASE("word vocabulary respects its configured cap") {
  const auto vocabulary = build_word_vocabulary({"alphabet alpha"}, {}, 1);

  REQUIRE(vocabulary.size() == 1);
  REQUIRE(word_completion_suffix("al", 2, vocabulary) == "phabet");
}
