#pragma once

#include <cstddef>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace booth {

struct WordCandidate {
  std::string word;
  bool from_history{false};
};

using WordVocabulary = std::vector<WordCandidate>;

/// Builds a ranked, case-insensitively unique vocabulary from prompts and transcript text.
/// History candidates rank before transcript candidates; ties use longer words, then lexical order.
WordVocabulary build_word_vocabulary(const std::vector<std::string>& history,
                                     const std::vector<std::string>& transcript,
                                     std::size_t max_words = 4000);

/// Returns the stored-spelling suffix for the best candidate at the cursor, or nullopt if none.
std::optional<std::string> word_completion_suffix(std::string_view prompt, std::size_t cursor,
                                                  const WordVocabulary& vocabulary);

}  // namespace booth
