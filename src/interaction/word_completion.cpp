#include "interaction/word_completion.hpp"

#include <algorithm>
#include <cctype>
#include <unordered_map>
#include <utility>

namespace booth {
namespace {

bool is_word_character(char ch) {
  const unsigned char byte = static_cast<unsigned char>(ch);
  return (byte >= 'A' && byte <= 'Z') || (byte >= 'a' && byte <= 'z') ||
         (byte >= '0' && byte <= '9') || ch == '_' || ch == '.' || ch == '/' || ch == ':' ||
         ch == '-';
}

char ascii_lower(char ch) {
  if (ch >= 'A' && ch <= 'Z') return static_cast<char>(ch - 'A' + 'a');
  return ch;
}

std::string folded(std::string_view word) {
  std::string result;
  result.reserve(word.size());
  for (const char ch : word) result.push_back(ascii_lower(ch));
  return result;
}

bool ranked_before(const WordCandidate& left, const WordCandidate& right) {
  if (left.from_history != right.from_history) return left.from_history;
  if (left.word.size() != right.word.size()) return left.word.size() > right.word.size();
  return left.word < right.word;
}

void prune(WordVocabulary& words, std::unordered_map<std::string, std::size_t>& indexes,
           std::size_t limit) {
  std::sort(words.begin(), words.end(), ranked_before);
  if (words.size() > limit) words.resize(limit);
  indexes.clear();
  indexes.reserve(words.size());
  for (std::size_t i = 0; i < words.size(); ++i) indexes.emplace(folded(words[i].word), i);
}

void add_text(const std::string& text, bool from_history, WordVocabulary& words,
              std::unordered_map<std::string, std::size_t>& indexes, std::size_t max_words) {
  const std::size_t threshold = max_words > static_cast<std::size_t>(-1) / 2
                                    ? static_cast<std::size_t>(-1)
                                    : max_words * 2;
  std::size_t at = 0;
  while (at < text.size()) {
    while (at < text.size() && !is_word_character(text[at])) ++at;
    const std::size_t start = at;
    while (at < text.size() && is_word_character(text[at])) ++at;
    if (at - start < 2) continue;

    std::string spelling = text.substr(start, at - start);
    const std::string key = folded(spelling);
    if (indexes.contains(key)) continue;
    words.push_back(WordCandidate{std::move(spelling), from_history});
    indexes.emplace(key, words.size() - 1);
    if (words.size() > threshold) prune(words, indexes, max_words);
  }
}

}  // namespace

WordVocabulary build_word_vocabulary(const std::vector<std::string>& history,
                                     const std::vector<std::string>& transcript,
                                     std::size_t max_words) {
  if (max_words == 0) return {};

  WordVocabulary words;
  std::unordered_map<std::string, std::size_t> indexes;
  indexes.reserve(std::min(max_words * (max_words <= static_cast<std::size_t>(-1) / 2 ? 2 : 1),
                           static_cast<std::size_t>(8000)));
  for (const auto& prompt : history) add_text(prompt, true, words, indexes, max_words);
  prune(words, indexes, max_words);

  if (words.size() < max_words) {
    for (const auto& entry : transcript) add_text(entry, false, words, indexes, max_words);
  }
  prune(words, indexes, max_words);
  return words;
}

std::optional<std::string> word_completion_suffix(std::string_view prompt, std::size_t cursor,
                                                  const WordVocabulary& vocabulary) {
  cursor = std::min(cursor, prompt.size());
  std::size_t start = cursor;
  while (start > 0 && is_word_character(prompt[start - 1])) --start;
  const std::string_view prefix = prompt.substr(start, cursor - start);
  if (prefix.size() < 2) return std::nullopt;

  const std::string folded_prefix = folded(prefix);
  for (const auto& candidate : vocabulary) {
    if (candidate.word.size() < prefix.size()) continue;
    bool matches = true;
    for (std::size_t i = 0; i < prefix.size(); ++i) {
      if (ascii_lower(candidate.word[i]) != folded_prefix[i]) {
        matches = false;
        break;
      }
    }
    if (!matches) continue;
    if (candidate.word.size() == prefix.size()) return std::nullopt;
    return candidate.word.substr(prefix.size());
  }
  return std::nullopt;
}

}  // namespace booth
