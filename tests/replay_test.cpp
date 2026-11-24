#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <sstream>

#include "interaction/replay.hpp"

using booth::EntryKind;
using booth::parse_session_jsonl;

TEST_CASE("session jsonl keeps user, reasoning and assistant text") {
  std::istringstream input(R"(
{"type":"session","data":{}}
{"type":"user/message","data":{"source":{"kind":"user"},"content":[{"type":"text","text":"question"}]}}
{"type":"user/message","data":{"source":{"kind":"runtime-context"},"content":[{"type":"text","text":"skip"}]}}
{"type":"assistant/message","data":{"message":{"content":[{"type":"reasoning","text":"think"},{"type":"text","text":"answer"},{"type":"tool-call","name":"bash"}]}}}
{"type":"assistant/message","data":{"message":{"content":[{"type":"text","text":"more"}]}}}
)");
  const auto entries = parse_session_jsonl(input);
  REQUIRE(entries.size() == 4);
  REQUIRE(entries[0].kind == EntryKind::User);
  REQUIRE(entries[0].text == "question");
  REQUIRE(entries[1].kind == EntryKind::Thought);
  REQUIRE(entries[1].text == "think");
  REQUIRE(entries[2].kind == EntryKind::Assistant);
  REQUIRE(entries[2].text == "answer");
  REQUIRE(entries[3].text == "more");
}

TEST_CASE("session jsonl keeps the last entries under a limit") {
  std::istringstream input(R"(
{"type":"user/message","data":{"source":{"kind":"user"},"content":[{"type":"text","text":"one"}]}}
{"type":"user/message","data":{"source":{"kind":"user"},"content":[{"type":"text","text":"two"}]}}
{"type":"user/message","data":{"source":{"kind":"user"},"content":[{"type":"text","text":"three"}]}}
)");
  const auto entries = parse_session_jsonl(input, 2);
  REQUIRE(entries.size() == 2);
  REQUIRE(entries[0].text == "two");
  REQUIRE(entries[1].text == "three");
}

TEST_CASE("session jsonl replays tool calls with input, output and timing") {
  std::istringstream input(R"(
{"type":"tool/call","time":1000,"data":{"callId":"c1","name":"bash","arguments":"{\"command\":\"ls\"}"}}
{"type":"tool/result","time":1033,"data":{"message":{"toolCallId":"c1","isError":false,"content":[{"type":"text","text":"alpha\nbeta"}]}}}
{"type":"assistant/message","data":{"message":{"content":[{"type":"text","text":"done"}]}}}
)");
  const auto entries = parse_session_jsonl(input);
  REQUIRE(entries.size() == 2);
  REQUIRE(entries[0].kind == EntryKind::Tool);
  REQUIRE(entries[0].id == "c1");
  REQUIRE(entries[0].tool == "bash");
  REQUIRE(entries[0].input == "ls");
  REQUIRE(entries[0].output == "alpha\nbeta");
  REQUIRE(entries[0].status == "completed");
  REQUIRE(entries[0].finished > entries[0].started);
  REQUIRE(entries[1].text == "done");
}

TEST_CASE("nested tool-result content blocks are flattened") {
  std::istringstream input(R"(
{"type":"tool/call","time":1,"data":{"callId":"c1","name":"bash","arguments":"ls"}}
{"type":"tool/result","time":2,"data":{"message":{"toolCallId":"c1","isError":false,"content":[{"type":"content","content":{"type":"text","text":"nested output"}}]}}}
)");
  const auto entries = parse_session_jsonl(input);
  REQUIRE(entries.size() == 1);
  REQUIRE(entries[0].output == "nested output");
}

TEST_CASE("session summary reads the title and first user prompt") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-summary-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  std::filesystem::create_directories(dir / "s-1");
  const auto plain = dir / "session.jsonl";
  {
    std::ofstream out(plain);
    out << R"({"type":"session/title","data":{"title":"stored title"}})" << '\n';
    out << R"({"type":"user/message","data":{"source":{"kind":"user"},"content":[{"type":"text","text":"first question\nsecond line"}]}})" << '\n';
  }
  const auto compressed = dir / "s-1" / "session.v4.jsonl.zstd";
  const std::string command = "zstd -q -f -o '" + compressed.string() + "' '" +
                              plain.string() + "'";
  REQUIRE(std::system(command.c_str()) == 0);
  const auto summary = booth::session_summary(dir, "s-1");
  REQUIRE(summary.title == "stored title");
  REQUIRE(summary.preview == "first question");
  REQUIRE(summary.age_seconds >= 0);
  std::filesystem::remove_all(dir);
}

TEST_CASE("a missing session store is empty, not an error") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-replay-missing-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  const auto result = booth::replay_session(dir, "absent");
  REQUIRE(result.has_value());
  REQUIRE(result->empty());
}

TEST_CASE("a corrupt session store is a process error") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-replay-bad-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  std::filesystem::create_directories(dir / "s-bad");
  {
    std::ofstream out(dir / "s-bad" / "session.v4.jsonl.zstd");
    out << "not a zstd frame";
  }
  const auto result = booth::replay_session(dir, "s-bad");
  REQUIRE_FALSE(result.has_value());
  REQUIRE(result.error().layer == booth::Layer::Process);
  std::filesystem::remove_all(dir);
}
