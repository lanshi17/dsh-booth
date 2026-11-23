#include "interaction/mention.hpp"

#include <algorithm>
#include <cctype>
#include <cstdlib>
#include <fstream>

#include <nlohmann/json.hpp>

namespace booth {
namespace {

bool is_space(char ch) { return std::isspace(static_cast<unsigned char>(ch)) != 0; }

std::string lower(std::string_view text) {
  std::string out(text);
  for (auto& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

std::string trim_punctuation(std::string token) {
  while (!token.empty() && std::string(".,;:!?").find(token.back()) != std::string::npos) {
    token.pop_back();
  }
  return token;
}

std::string sanitize(std::string_view text) {
  std::string out;
  for (char ch : text) {
    if (ch == '"' || ch == '&' || ch == '<' || ch == '>') continue;
    out.push_back(ch);
  }
  return out;
}

struct Route {
  std::string provider;
  std::string model;
};

Route route_of(const ModelChoice& choice) {
  const auto parsed = nlohmann::json::parse(choice.value, nullptr, false);
  if (parsed.is_array() && parsed.size() >= 2 && parsed[0].is_string() && parsed[1].is_string()) {
    return {parsed[0].get<std::string>(), parsed[1].get<std::string>()};
  }
  return {choice.provider, choice.name};
}

std::string selector_of(const ModelChoice& choice) {
  const auto route = route_of(choice);
  return route.provider + "/" + route.model;
}

bool effort_allowed(std::string_view effort, const std::vector<std::string>& efforts) {
  return std::find(efforts.begin(), efforts.end(), effort) != efforts.end();
}

std::optional<ModelMention> resolve(std::string_view token,
                                    const std::vector<ModelChoice>& models,
                                    const std::vector<std::string>& efforts) {
  for (const auto& choice : models) {
    const auto route = route_of(choice);
    const std::string selector = selector_of(choice);
    if (token == choice.name || token == selector || token == choice.value) {
      return ModelMention{"", selector, route.provider, route.model, "",
                          choice.name};
    }
    const auto colon = token.find_last_of(':');
    if (colon == std::string_view::npos) continue;
    const auto base = token.substr(0, colon);
    const auto effort = std::string(token.substr(colon + 1));
    if (base != selector && base != choice.name) continue;
    if (!effort_allowed(effort, efforts)) continue;
    return ModelMention{"", selector + ":" + effort, route.provider, route.model, effort,
                        choice.name + " · " + effort};
  }
  return std::nullopt;
}

}  // namespace

std::string MentionRegistry::expand(std::string_view text,
                                    const std::vector<ModelChoice>& models,
                                    const std::vector<std::string>& efforts) {
  std::string out;
  out.reserve(text.size());
  std::size_t index = 0;
  while (index < text.size()) {
    if (text[index] != '^') {
      out.push_back(text[index++]);
      continue;
    }
    const bool boundary = index == 0 || is_space(text[index - 1]);
    const auto end = text.find_first_of(" \t\r\n", index + 1);
    const auto stop = end == std::string_view::npos ? text.size() : end;
    if (!boundary || index + 1 >= stop) {
      out.push_back(text[index++]);
      continue;
    }
    const std::string raw(text.substr(index + 1, stop - index - 1));
    auto mention = resolve(raw, models, efforts);
    std::string suffix;
    if (!mention) {
      const auto trimmed = trim_punctuation(raw);
      if (trimmed.size() != raw.size()) {
        mention = resolve(trimmed, models, efforts);
        suffix = raw.substr(trimmed.size());
      }
    }
    if (!mention) {
      out.push_back(text[index++]);
      continue;
    }
    auto found = by_selector_.find(mention->selector);
    if (found == by_selector_.end()) {
      mention->agent = "m" + std::to_string(mentions_.size() + 1);
      mentions_.push_back(*mention);
      found = by_selector_.emplace(mention->selector, *mention).first;
    }
    out += "<model agent=\"" + found->second.agent + "\" name=\"" +
           sanitize(found->second.name) + "\" provider=\"" + sanitize(found->second.provider) +
           "\" model=\"" + sanitize(found->second.model) + "\"";
    if (!found->second.effort.empty()) out += " reasoning_effort=\"" + sanitize(found->second.effort) + "\"";
    out += "/>";
    out += suffix;
    index = stop;
  }
  return out;
}

std::string MentionRegistry::preamble() const {
  if (mentions_.empty()) return {};
  std::string out = "[booth model mentions for this turn:\n";
  for (const auto& mention : mentions_) {
    out += mention.agent + ": provider=\"" + mention.provider + "\", model=\"" +
           mention.model + "\", name=\"" + sanitize(mention.name) + "\"";
    if (!mention.effort.empty()) out += ", reasoning_effort=\"" + mention.effort + "\"";
    out += "\n";
  }
  out += "When the user asks one of these aliases to do work, call the subagent (task) tool with "
         "that alias's provider, model and reasoning_effort.]";
  return out;
}

void MentionRegistry::clear() {
  mentions_.clear();
  by_selector_.clear();
}

void MentionRegistry::save(const std::filesystem::path& path) const {
  nlohmann::json rows = nlohmann::json::array();
  for (const auto& mention : mentions_) {
    rows.push_back({{"agent", mention.agent}, {"selector", mention.selector},
                    {"provider", mention.provider}, {"model", mention.model},
                    {"effort", mention.effort}, {"name", mention.name}});
  }
  std::ofstream out(path, std::ios::trunc);
  if (out) out << rows.dump(2);
}

void MentionRegistry::load(const std::filesystem::path& path) {
  clear();
  std::ifstream in(path);
  if (!in) return;
  const auto rows = nlohmann::json::parse(in, nullptr, false);
  if (!rows.is_array()) return;
  for (const auto& row : rows) {
    if (!row.is_object()) continue;
    ModelMention mention{row.value("agent", std::string{}), row.value("selector", std::string{}),
                         row.value("provider", std::string{}), row.value("model", std::string{}),
                         row.value("effort", std::string{}), row.value("name", std::string{})};
    if (mention.agent.empty() || mention.selector.empty()) continue;
    by_selector_[mention.selector] = mention;
    mentions_.push_back(std::move(mention));
  }
}

std::optional<MentionQuery> active_mention(std::string_view text, std::size_t cursor) {
  cursor = std::min(cursor, text.size());
  std::size_t start = cursor;
  while (start > 0 && !is_space(text[start - 1]) && text[start - 1] != '^') --start;
  if (start == 0 || text[start - 1] != '^') return std::nullopt;
  if (start > 1 && !is_space(text[start - 2])) return std::nullopt;
  return MentionQuery{start - 1, cursor, std::string(text.substr(start, cursor - start))};
}

std::vector<MentionSuggestion> mention_suggestions(std::string_view query,
                                                   const std::vector<ModelChoice>& models,
                                                   const std::vector<std::string>& efforts,
                                                   std::size_t limit) {
  const std::string lowered = lower(query);
  std::vector<MentionSuggestion> out;
  const auto colon = query.find_last_of(':');
  if (colon != std::string_view::npos) {
    const auto base = query.substr(0, colon);
    for (const auto& model : models) {
      const auto selector = selector_of(model);
      if (lower(selector).find(lower(base)) == std::string::npos) continue;
      for (const auto& effort : efforts) {
        const std::string full = selector + ":" + effort;
        if (lower(full).find(lowered) == std::string::npos) continue;
        out.push_back({full, model.name + " · " + effort});
        if (out.size() == limit) return out;
      }
    }
    if (!out.empty()) return out;
  }
  for (const auto& model : models) {
    const auto selector = selector_of(model);
    if (!lowered.empty() && lower(selector).find(lowered) == std::string::npos &&
        lower(model.name).find(lowered) == std::string::npos) {
      continue;
    }
    out.push_back({selector, model.name});
    if (out.size() == limit) break;
  }
  return out;
}

std::pair<std::string, std::size_t> apply_mention(std::string text, const MentionQuery& query,
                                                  std::string_view selector) {
  const std::string replacement = "^" + std::string(selector) + " ";
  text.replace(query.start, query.end - query.start, replacement);
  return {std::move(text), query.start + replacement.size()};
}

std::optional<ModelChoice> take_model_mention(std::string& text,
                                              const std::vector<ModelChoice>& models) {
  if (text.empty() || text.front() != '^') return std::nullopt;
  const auto end = text.find_first_of(" \t\r\n");
  const std::string token = text.substr(1, end == std::string::npos ? std::string::npos : end - 1);
  for (const auto& model : models) {
    if (token != model.name && token != model.value && token != selector_of(model)) continue;
    std::size_t next = end == std::string::npos ? text.size() : end;
    while (next < text.size() && is_space(text[next])) ++next;
    text.erase(0, next);
    return model;
  }
  return std::nullopt;
}

}  // namespace booth
