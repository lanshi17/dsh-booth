#include <catch2/catch_test_macros.hpp>

#include <filesystem>
#include <fstream>
#include <string>

#include <unistd.h>

#include "interaction/skill_catalog.hpp"

using namespace booth;

namespace {
std::filesystem::path temp_root() {
  const auto root = std::filesystem::temp_directory_path() /
                    ("booth-skills-" + std::to_string(::getpid()));
  std::filesystem::remove_all(root);
  std::filesystem::create_directories(root);
  return root;
}

void write_file(const std::filesystem::path& path, const std::string& body) {
  std::filesystem::create_directories(path.parent_path());
  std::ofstream(path, std::ios::trunc) << body;
}
}  // namespace

TEST_CASE("skills parse frontmatter and honor user-invocable") {
  const auto root = temp_root();
  const auto bundle = root / ".dsh/skills/review/SKILL.md";
  write_file(bundle, "---\nname: review\ndescription: Review a change\n---\nBody\n");
  const auto flat = root / ".agents/skills/fix.md";
  write_file(flat, "---\nname: fix\ndescription: Fix tests\nuser-invocable: false\n---\nBody\n");

  const auto skills = scan_skills(root, root / "home", root / "agents");
  REQUIRE(skills.size() == 1);
  REQUIRE(skills[0].name == "review");
  REQUIRE(skills[0].description == "Review a change");
  REQUIRE(skills[0].source == "project .dsh");
  std::filesystem::remove_all(root);
}

TEST_CASE("project skills win over user skills and user agents are last") {
  const auto root = temp_root();
  write_file(root / ".dsh/skills/shared/SKILL.md",
             "---\nname: shared\ndescription: project\n---\n");
  write_file(root / "dsh/skills/shared/SKILL.md",
             "---\nname: shared\ndescription: user\n---\n");
  write_file(root / "agents/skills/extra/SKILL.md",
             "---\nname: extra\ndescription: agents\n---\n");

  const auto skills = scan_skills(root, root / "dsh", root / "agents");
  REQUIRE(skills.size() == 2);
  REQUIRE(skills[0].name == "shared");
  REQUIRE(skills[0].description == "project");
  REQUIRE(skills[1].name == "extra");
  REQUIRE(skills[1].source == "user agents");
  std::filesystem::remove_all(root);
}
