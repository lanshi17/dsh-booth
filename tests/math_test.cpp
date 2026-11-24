#include <catch2/catch_test_macros.hpp>

#include <string>

#include "interaction/math.hpp"

using namespace booth;

TEST_CASE("latex converter handles common math") {
  REQUIRE(latex_to_unicode("E = mc^2") == "E = mc²");
  REQUIRE(latex_to_unicode("\\frac{1}{2}") == "½");
  REQUIRE(latex_to_unicode("\\frac{a+b}{c}") == "(a+b)/c");
  REQUIRE(latex_to_unicode("\\sqrt{x}") == "√x");
  REQUIRE(latex_to_unicode("\\sqrt[3]{x}") == "∛x");
  REQUIRE(latex_to_unicode("\\alpha + \\beta") == "α + β");
  REQUIRE(latex_to_unicode("x^{10}") == "x¹⁰");
  REQUIRE(latex_to_unicode("a_{i+1}") == "aᵢ₊₁");
  REQUIRE(latex_to_unicode("\\text{speed}") == "speed");
  REQUIRE(latex_to_unicode("\\left( x \\right)") == "( x )");
}

TEST_CASE("math spans reject currency and honor delimiters") {
  const auto dollar = math_span_at("$E=mc^2$", 0);
  REQUIRE(dollar.has_value());
  REQUIRE(dollar->text == "E=mc²");
  REQUIRE(dollar->end == 8);

  const auto display = math_span_at("$$\\frac{1}{2}$$", 0);
  REQUIRE(display.has_value());
  REQUIRE(display->text == "½");
  REQUIRE(display->end == 15);

  const auto paren = math_span_at("\\(a+b\\)", 0);
  REQUIRE(paren.has_value());
  REQUIRE(paren->text == "a+b");
  REQUIRE(paren->end == 7);

  REQUIRE_FALSE(math_span_at("$5 and $10", 0));
  REQUIRE_FALSE(math_span_at("$x$2", 0));
  REQUIRE_FALSE(math_span_at("$ x$", 0));
}
