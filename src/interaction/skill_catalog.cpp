#include "interaction/skill_catalog.hpp"

#include <cstdlib>
#include <fstream>
#include <map>
#include <optional>
#include <sstream>

namespace booth {
namespace {

std::string trim(std::string_view text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string_view::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return std::string(text.substr(first, last - first + 1));
}

std::string unquote(std::string value) {
  if (value.size() >= 2 && ((value.front() == '"' && value.back() == '"') ||
                            (value.front() == '\'' && value.back() == '\''))) {
    value = value.substr(1, value.size() - 2);
  }
  return value;
}

std::optional<bool> yaml_bool(std::string value) {
  for (char& ch : value) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  if (value == "true" || value == "yes" || value == "on" || value == "1") return true;
  if (value == "false" || value == "no" || value == "off" || value == "0") return false;
  return std::nullopt;
}

std::map<std::string, std::string> frontmatter(std::string& body) {
  std::map<std::string, std::string> fields;
  if (body.rfind("---", 0) != 0) return fields;
  const auto end = body.find("\n---", 3);
  if (end == std::string::npos) return fields;
  std::istringstream input(body.substr(3, end - 3));
  std::string line;
  while (std::getline(input, line)) {
    const auto colon = line.find(':');
    if (colon == std::string::npos) continue;
    fields[trim(std::string_view(line).substr(0, colon))] =
        unquote(trim(std::string_view(line).substr(colon + 1)));
  }
  body.erase(0, end + 4);
  return fields;
}

std::vector<SkillInfo> scan_root(const std::filesystem::path& root, std::string source,
                                 std::vector<SkillInfo>& out) {
  std::error_code error;
  for (const auto& entry : std::filesystem::directory_iterator(root, error)) {
    std::filesystem::path path;
    if (entry.is_directory()) {
      path = entry.path() / "SKILL.md";
    } else if (entry.is_regular_file() && entry.path().extension() == ".md") {
      path = entry.path();
    } else {
      continue;
    }
    auto skill = read_skill(path, source);
    if (!skill) continue;
    const bool seen = std::any_of(out.begin(), out.end(),
                                  [&](const SkillInfo& item) { return item.name == skill->name; });
    if (!seen) out.push_back(std::move(*skill));
  }
  return out;
}

}  // namespace

std::optional<SkillInfo> read_skill(const std::filesystem::path& path, std::string source) {
  std::ifstream input(path);
  if (!input) return std::nullopt;
  std::string body((std::istreambuf_iterator<char>(input)), std::istreambuf_iterator<char>());
  const auto fields = frontmatter(body);
  const auto name = fields.find("name");
  const auto description = fields.find("description");
  if (name == fields.end() || description == fields.end()) return std::nullopt;
  if (const auto invocable = fields.find("user-invocable"); invocable != fields.end()) {
    const auto value = yaml_bool(invocable->second);
    if (!value || !*value) return std::nullopt;
  }
  if (name->second.empty() || description->second.empty()) return std::nullopt;
  return SkillInfo{name->second, description->second, path, std::move(source)};
}

std::filesystem::path project_root(const std::filesystem::path& cwd) {
  std::error_code error;
  auto current = std::filesystem::absolute(cwd, error);
  if (error) current = cwd;
  for (auto probe = current; !probe.empty(); probe = probe.parent_path()) {
    if (std::filesystem::exists(probe / ".git", error)) return probe;
    const auto parent = probe.parent_path();
    if (parent == probe) break;
  }
  return current;
}

std::vector<SkillInfo> scan_skills(const std::filesystem::path& project,
                                   const std::filesystem::path& dsh_home,
                                   const std::filesystem::path& agents_home) {
  std::vector<SkillInfo> skills;
  const auto root = project_root(project);
  scan_root(root / ".dsh" / "skills", "project .dsh", skills);
  scan_root(root / ".agents" / "skills", "project .agents", skills);
  scan_root(dsh_home / "skills", "user dsh", skills);
  scan_root(agents_home / "skills", "user agents", skills);
  return skills;
}

std::vector<SkillInfo> scan_default_skills(const std::filesystem::path& cwd) {
  const auto home = std::getenv("HOME") == nullptr ? std::filesystem::path{} : std::filesystem::path(std::getenv("HOME"));
  const auto env_path = [&](const char* name, const std::filesystem::path& fallback) {
    const char* value = std::getenv(name);
    return value != nullptr && *value != '\0' ? std::filesystem::path(value) : fallback;
  };
  return scan_skills(cwd, env_path("DSH_HOME", home / ".dsh"), env_path("DSH_AGENTS_HOME", home / ".agents"));
}

}  // namespace booth
