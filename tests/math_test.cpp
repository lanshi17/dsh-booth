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

TEST_CASE("prose math replacement leaves currency alone") {
  REQUIRE(render_math_in_text("value $x^2$") == "value x²");
  REQUIRE(render_math_in_text("costs $5 and $10") == "costs $5 and $10");
  REQUIRE(render_math_in_text("\\$5 stays literal") == "$5 stays literal");
}

TEST_CASE("display fractions stack over a bar") {
  const auto lines = math_block_lines("\\frac{a+b}{c}");
  REQUIRE(lines.size() == 3);
  REQUIRE(lines[0].find("a+b") != std::string::npos);
  REQUIRE(lines[1].find("───") != std::string::npos);
  REQUIRE(lines[2].find("c") != std::string::npos);
}

TEST_CASE("matrix environments render a grid") {
  const auto lines = math_block_lines("\\begin{pmatrix}1&2\\\\3&4\\end{pmatrix}");
  REQUIRE(lines.size() == 2);
  REQUIRE(lines[0].find("⎛") != std::string::npos);
  REQUIRE(lines[0].find("1") != std::string::npos);
  REQUIRE(lines[0].find("2") != std::string::npos);
  REQUIRE(lines[1].find("⎝") != std::string::npos);
  REQUIRE(lines[1].find("3") != std::string::npos);
  REQUIRE(lines[1].find("4") != std::string::npos);
}

TEST_CASE("other display math falls back to one line") {
  const auto lines = math_block_lines("E = mc^2");
  REQUIRE(lines.size() == 1);
  REQUIRE(lines[0].find("E = mc²") != std::string::npos);
}

TEST_CASE("display radicals draw a top bar") {
  const auto root = math_block_lines("\\sqrt{x+1}");
  REQUIRE(root.size() == 2);
  REQUIRE(root[0].find("┌───") != std::string::npos);
  REQUIRE(root[1].find("√ x+1") != std::string::npos);

  const auto cube = math_block_lines("\\sqrt[3]{x}");
  REQUIRE(cube.size() == 2);
  REQUIRE(cube[1].find("∛ x") != std::string::npos);
}

TEST_CASE("nested fractions stack vertically") {
  const auto lines = math_block_lines("\\frac{\\frac{a}{b}}{c}");
  REQUIRE(lines.size() == 5);
  REQUIRE(lines[0].find("a") != std::string::npos);
  REQUIRE(lines[1].find("─") != std::string::npos);
  REQUIRE(lines[2].find("b") != std::string::npos);
  REQUIRE(lines[3].find("─") != std::string::npos);
  REQUIRE(lines[4].find("c") != std::string::npos);
}

TEST_CASE("latex accents attach combining marks") {
  REQUIRE(latex_to_unicode("\\hat{x}") == "x\u0302");
  REQUIRE(latex_to_unicode("\\bar{x}") == "x\u0304");
  REQUIRE(latex_to_unicode("\\vec{v}") == "v\u20d7");
  REQUIRE(latex_to_unicode("\\dot{x}") == "x\u0307");
  REQUIRE(latex_to_unicode("\\ddot{x}") == "x\u0308");
  REQUIRE(latex_to_unicode("\\tilde{x}") == "x\u0303");
  REQUIRE(latex_to_unicode("\\overline{AB}") == "AB\u0304");
}

TEST_CASE("latex binomials render inline and stacked") {
  REQUIRE(latex_to_unicode("\\binom{n}{k}") == "C(n,k)");
  const auto lines = math_block_lines("\\binom{n}{k}");
  REQUIRE(lines.size() == 3);
  REQUIRE(lines[0].find("⎛") != std::string::npos);
  REQUIRE(lines[0].find("n") != std::string::npos);
  REQUIRE(lines[1].find("─") != std::string::npos);
  REQUIRE(lines[2].find("⎝") != std::string::npos);
  REQUIRE(lines[2].find("k") != std::string::npos);
}
