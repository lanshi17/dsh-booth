#pragma once

#include <cstddef>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace booth {

struct MathSpan {
  std::size_t end{0};
  std::string text;
};

/// Converts a bare LaTeX math fragment to Unicode text.
std::string latex_to_unicode(std::string_view source);

/// Recognizes `$…$`, `$$…$$`, `\(…\)` and `\[…\]` at `at`.
/// Currency-like `$…$` spans are rejected; the caller handles escaped `\$`.
std::optional<MathSpan> math_span_at(const std::string& text, std::size_t at);

/// Replaces every recognized math span in prose with its Unicode rendering.
std::string render_math_in_text(const std::string& text);

/// Display math as rows: a top-level fraction stacks, matrix environments draw a grid,
/// everything else falls back to one Unicode line.
std::vector<std::string> math_block_lines(std::string_view source);

}  // namespace booth
