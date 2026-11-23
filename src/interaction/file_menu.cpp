#include "interaction/file_menu.hpp"

#include <algorithm>
#include <cctype>
#include <filesystem>
#include <system_error>

namespace booth {
namespace {

std::string lower(std::string_view text) {
  std::string out(text);
  for (auto& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

bool is_space(char ch) { return std::isspace(static_cast<unsigned char>(ch)) != 0; }

bool at_token_start(const std::string& text, std::size_t at) {
  return at == 0 || is_space(text[at - 1]);
}

std::size_t find_token(const std::string& text) {
  if (text.empty()) return std::string::npos;
  for (std::size_t at = text.size(); at-- > 0;) {
    if (text[at] != '@' || !at_token_start(text, at)) continue;
    if (text.find_first_of(" \t\r\n", at + 1) != std::string::npos) return std::string::npos;
    return at;
  }
  return std::string::npos;
}

bool skip_dir(const std::filesystem::path& path) {
  const std::string name = path.filename().string();
  return name == ".git" || name == "build" || name == "build-asan" || name == "build-release" ||
         name == "node_modules" || name == "target";
}

}  // namespace

std::vector<std::string> complete_files(const std::string& cwd, const std::string& prefix,
                                        std::size_t limit) {
  std::vector<std::string> matches;
  std::error_code error;
  const std::filesystem::path root(cwd);
  if (!std::filesystem::is_directory(root, error)) return matches;
  const std::string needle = lower(prefix);
  std::size_t visited = 0;
  for (auto entry = std::filesystem::recursive_directory_iterator(
           root, std::filesystem::directory_options::skip_permission_denied, error);
       entry != std::filesystem::recursive_directory_iterator(); entry.increment(error)) {
    if (error) {
      error.clear();
      continue;
    }
    if (++visited > 20000) break;
    const auto& path = entry->path();
    if (entry->is_directory(error)) {
      if (skip_dir(path)) entry.disable_recursion_pending();
      continue;
    }
    if (!entry->is_regular_file(error)) continue;
    const std::string relative = path.lexically_relative(root).generic_string();
    if (!needle.empty() && lower(relative).find(needle) == std::string::npos) continue;
    matches.push_back(relative);
    if (matches.size() >= limit) break;
  }
  std::sort(matches.begin(), matches.end());
  return matches;
}

void FileMenu::sync(const std::string& composer, const std::string& cwd) {
  if (composer == seen_) return;
  seen_ = composer;
  const std::size_t at = find_token(composer);
  if (at == std::string::npos) {
    visible_ = false;
    matches_.clear();
    query_.clear();
    return;
  }
  token_start_ = at;
  query_ = composer.substr(at + 1);
  index_ = 0;
  matches_ = complete_files(cwd, query_);
  visible_ = !matches_.empty();
}

void FileMenu::close() {
  visible_ = false;
  matches_.clear();
  seen_.clear();
}

bool FileMenu::key(ftxui::Event& event, std::string& composer) {
  if (!visible_) return false;
  if (event == ftxui::Event::Escape) {
    close();
    return true;
  }
  if (event == ftxui::Event::ArrowUp ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelUp)) {
    index_ = (index_ + matches_.size() - 1) % matches_.size();
    return true;
  }
  if (event == ftxui::Event::ArrowDown ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelDown)) {
    index_ = (index_ + 1) % matches_.size();
    return true;
  }
  if (event == ftxui::Event::Tab || event == ftxui::Event::Return) {
    composer = composer.substr(0, token_start_) + "@" + matches_[index_] + " ";
    close();
    return true;
  }
  return false;
}

bool FileMenu::click(std::size_t row, std::string& composer) {
  if (!visible_ || row >= matches_.size()) return false;
  index_ = row;
  composer = composer.substr(0, token_start_) + "@" + matches_[index_] + " ";
  close();
  return true;
}

std::vector<std::pair<std::string, std::string>> file_links(const std::string& text,
                                                            const std::string& cwd) {
  std::vector<std::pair<std::string, std::string>> links;
  for (std::size_t at = 0; at < text.size(); ++at) {
    if (text[at] != '@' || !at_token_start(text, at)) continue;
    std::size_t end = at + 1;
    while (end < text.size() && !is_space(text[end])) ++end;
    std::string path = text.substr(at + 1, end - at - 1);
    if (path.size() >= 2 && path.front() == '"' && path.back() == '"') {
      path = path.substr(1, path.size() - 2);
    }
    if (path.empty()) continue;
    std::error_code error;
    auto absolute = std::filesystem::absolute(std::filesystem::path(cwd) / path, error);
    if (error || !std::filesystem::exists(absolute, error)) continue;
    absolute = absolute.lexically_normal();
    const std::string uri = "file://" + absolute.string();
    if (std::none_of(links.begin(), links.end(),
                     [&](const auto& link) { return link.second == uri; })) {
      links.emplace_back(absolute.filename().string(), uri);
    }
  }
  return links;
}

}  // namespace booth
