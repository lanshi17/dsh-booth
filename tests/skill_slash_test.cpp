#include <catch2/catch_test_macros.hpp>

#include <algorithm>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <optional>
#include <string>
#include <vector>

#include <unistd.h>

#include "interaction/commands.hpp"
#include "interaction/skill_catalog.hpp"

using namespace booth;

namespace {

void write_file(const std::filesystem::path& path, const std::string& body) {
  std::filesystem::create_directories(path.parent_path());
  std::ofstream(path, std::ios::trunc) << body;
}

bool contains(const std::vector<const CommandHint*>& matches, std::string_view name) {
  return std::any_of(matches.begin(), matches.end(),
                     [&](const CommandHint* hint) { return name == hint->name; });
}

/// One temp project plus empty user roots, so the process-wide skill registry can be
/// installed and cleared without leaking into the other test cases.
struct SkillFixture {
  explicit SkillFixture(std::string tag)
      : root(std::filesystem::temp_directory_path() /
             ("booth-skill-slash-" + std::move(tag) + "-" + std::to_string(::getpid()))),
        old_dsh(env_value("DSH_HOME")), old_agents(env_value("DSH_AGENTS_HOME")) {
    std::filesystem::remove_all(root);
    for (const char* dir : {"home/skills", "agents/skills", "empty/.git"}) {
      std::filesystem::create_directories(root / dir);
    }
    ::setenv("DSH_HOME", (root / "home").c_str(), 1);
    ::setenv("DSH_AGENTS_HOME", (root / "agents").c_str(), 1);
  }
  ~SkillFixture() {
    load_skill_commands(root / "empty");  // clears gSkills while the env still points at empty roots
    restore("DSH_HOME", old_dsh);
    restore("DSH_AGENTS_HOME", old_agents);
    std::filesystem::remove_all(root);
  }
  SkillFixture(const SkillFixture&) = delete;
  SkillFixture& operator=(const SkillFixture&) = delete;

  std::filesystem::path project(const std::string& name) {
    const auto dir = root / name;
    std::filesystem::create_directories(dir / ".git");
    return dir;
  }

  std::filesystem::path root;

 private:
  std::optional<std::string> old_dsh;
  std::optional<std::string> old_agents;

  static std::optional<std::string> env_value(const char* name) {
    const char* value = ::getenv(name);
    return value == nullptr ? std::nullopt : std::optional<std::string>(value);
  }
  static void restore(const char* name, const std::optional<std::string>& value) {
    if (value) ::setenv(name, value->c_str(), 1);
    else ::unsetenv(name);
  }
};

}  // namespace

TEST_CASE("a skill is a slash hint only when its name matches") {
  SkillFixture fixture("match");
  const auto project = fixture.project("project");
  write_file(project / ".dsh/skills/review/SKILL.md",
             "---\nname: review\ndescription: Review a change\n---\nReview the change.\n");

  const auto before = command_hints().size();
  const auto loaded = load_skill_commands(project);
  REQUIRE(loaded.size() == 1);
  REQUIRE(loaded.front().name == "review");
  REQUIRE(command_hints().size() == before + 1);

  // Every skill used to match every query, which flooded the menu with unrelated names.
  REQUIRE(matching_commands("/zzz").empty());

  const auto partial = matching_commands("/rev");
  REQUIRE(partial.size() >= 1);
  REQUIRE(partial.front()->name == std::string("/review"));

  const auto exact = matching_commands("/review");
  REQUIRE(exact.front()->name == std::string("/review"));

  // Skill descriptions are not searchable text; a large catalog must not flood the menu.
  REQUIRE_FALSE(contains(matching_commands("/change"), "/review"));
}

TEST_CASE("commands rank before skills only when their scores tie") {
  SkillFixture fixture("tie");
  const auto project = fixture.project("project");
  write_file(project / ".dsh/skills/review/SKILL.md",
             "---\nname: review\ndescription: Review a change\n---\n");

  load_skill_commands(project);
  const auto matches = matching_commands("/");
  REQUIRE(matches.size() == command_hints().size());
  REQUIRE(matches.front()->name == std::string("/help"));
  REQUIRE_FALSE(matches.front()->skill);
  REQUIRE(matches.back()->name == std::string("/review"));
  REQUIRE(matches.back()->skill);
}

TEST_CASE("a skill prompt is a registered slash token, not a local command") {
  SkillFixture fixture("prompt");
  const auto project = fixture.project("project");
  write_file(project / ".dsh/skills/review/SKILL.md",
             "---\nname: review\ndescription: Review a change\n---\n");

  load_skill_commands(project);
  REQUIRE(is_skill_prompt("/review"));
  REQUIRE(is_skill_prompt("  /REVIEW please check this"));
  REQUIRE_FALSE(is_skill_prompt("/reviews"));
  REQUIRE_FALSE(is_skill_prompt("/review2"));
  REQUIRE_FALSE(is_skill_prompt("/model"));
  REQUIRE_FALSE(is_skill_prompt("review"));
  REQUIRE_FALSE(is_skill_prompt(""));
}

TEST_CASE("skills that shadow a command name or alias are not registered") {
  SkillFixture fixture("conflict");
  const auto project = fixture.project("project");
  write_file(project / ".dsh/skills/copy/SKILL.md",
             "---\nname: copy\ndescription: shadow a command name\n---\n");
  write_file(project / ".dsh/skills/exit/SKILL.md",
             "---\nname: exit\ndescription: shadow a command alias\n---\n");
  write_file(project / ".dsh/skills/keep/SKILL.md",
             "---\nname: keep\ndescription: a real skill\n---\n");

  const auto before = command_hints().size();
  const auto loaded = load_skill_commands(project);
  REQUIRE(loaded.size() == 1);
  REQUIRE(loaded.front().name == "keep");
  REQUIRE(command_hints().size() == before + 1);
  REQUIRE_FALSE(is_skill_prompt("/copy"));
  REQUIRE_FALSE(is_skill_prompt("/exit"));
  REQUIRE(is_skill_prompt("/keep"));
}

TEST_CASE("the loader dedupes skills and returns the project one first") {
  SkillFixture fixture("dedupe");
  const auto project = fixture.project("project");
  write_file(project / ".dsh/skills/review/SKILL.md",
             "---\nname: review\ndescription: project copy\n---\n");
  write_file(project / ".agents/skills/review/SKILL.md",
             "---\nname: review\ndescription: agents copy\n---\n");
  write_file(fixture.root / "home/skills/user-one/SKILL.md",
             "---\nname: user-one\ndescription: user copy\n---\n");

  const auto loaded = load_skill_commands(project);
  REQUIRE(loaded.size() == 2);
  REQUIRE(loaded[0].name == "review");
  REQUIRE(loaded[0].description == "project copy");
  REQUIRE(loaded[1].name == "user-one");
  REQUIRE(loaded[1].source == "user dsh");
}

TEST_CASE("a skill body larger than the frontmatter budget still parses") {
  SkillFixture fixture("large");
  const auto project = fixture.project("project");
  write_file(project / ".dsh/skills/big/SKILL.md",
             "---\nname: big\ndescription: Big skill\n---\n" + std::string(100'000, 'x') + "\n");

  const auto skills = scan_default_skills(project);
  REQUIRE(skills.size() == 1);
  REQUIRE(skills.front().name == "big");
  REQUIRE(skills.front().description == "Big skill");
}
