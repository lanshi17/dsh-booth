#pragma once

#include <filesystem>
#include <string>
#include <vector>

namespace booth {

/// One user-invocable dsh skill discovered from disk.
struct SkillInfo {
  std::string name;
  std::string description;
  std::filesystem::path path;
  std::string source;
};

/// Parses one `SKILL.md` or flat `<name>.md`; nullopt when invalid or user-invocable is false.
std::optional<SkillInfo> read_skill(const std::filesystem::path& path, std::string source);

/// Scans project `.dsh/skills`, project `.agents/skills`, user dsh, then user agents roots.
std::vector<SkillInfo> scan_skills(const std::filesystem::path& project,
                                   const std::filesystem::path& dsh_home,
                                   const std::filesystem::path& agents_home);

/// Finds the nearest ancestor containing `.git`, falling back to cwd.
std::filesystem::path project_root(const std::filesystem::path& cwd);

/// Scans the default roots for this process (`$DSH_HOME`, `$DSH_AGENTS_HOME`, home fallbacks).
std::vector<SkillInfo> scan_default_skills(const std::filesystem::path& cwd);

}  // namespace booth
