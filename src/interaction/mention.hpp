#pragma once

#include <cstddef>
#include <filesystem>
#include <map>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include "contract/options.hpp"

namespace booth {

/// A user-tagged model usable as an `mN` delegation alias in one session.
struct ModelMention {
  std::string agent;
  std::string selector;
  std::string provider;
  std::string model;
  std::string effort;
  std::string name;
};

/// Stable per-session mN aliases created from `^selector` mentions.
class MentionRegistry {
 public:
  /// Replaces known `^selector` tokens with `<model agent="mN" name="..."/>` tags.
  std::string expand(std::string_view text, const std::vector<ModelChoice>& models,
                     const std::vector<std::string>& efforts = {});
  /// Instruction block the kernel needs to turn mN tags into subagent model routes.
  [[nodiscard]] std::string preamble() const;
  [[nodiscard]] const std::vector<ModelMention>& mentions() const { return mentions_; }
  void clear();
  void save(const std::filesystem::path& path) const;
  void load(const std::filesystem::path& path);

 private:
  std::vector<ModelMention> mentions_;
  std::map<std::string, ModelMention> by_selector_;
};

/// The `^` token under the cursor, including surrounding delimiter bounds.
struct MentionQuery {
  std::size_t start;
  std::size_t end;
  std::string prefix;
};

std::optional<MentionQuery> active_mention(std::string_view text, std::size_t cursor);

struct MentionSuggestion {
  std::string selector;
  std::string name;
};

std::vector<MentionSuggestion> mention_suggestions(std::string_view query,
                                                   const std::vector<ModelChoice>& models,
                                                   const std::vector<std::string>& efforts = {},
                                                   std::size_t limit = 20);

/// Replaces the active token with `^selector ` and returns the new cursor offset.
std::pair<std::string, std::size_t> apply_mention(std::string text, const MentionQuery& query,
                                                  std::string_view selector);

/// Backward-compatible leading mention resolver retained for callers during migration.
std::optional<ModelChoice> take_model_mention(std::string& text,
                                              const std::vector<ModelChoice>& models);

}  // namespace booth
