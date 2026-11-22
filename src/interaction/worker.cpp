#include "interaction/worker.hpp"

#include <utility>

namespace booth {

Worker::Worker() : thread_([this] { loop(); }) {}

Worker::~Worker() { stop(); }

void Worker::post(std::function<void()> job) {
  {
    std::lock_guard<std::mutex> lock(mutex_);
    jobs_.push_back(std::move(job));
  }
  ready_.notify_one();
}

void Worker::stop() {
  {
    std::lock_guard<std::mutex> lock(mutex_);
    stopping_ = true;
  }
  ready_.notify_all();
  if (thread_.joinable()) thread_.join();
}

void Worker::loop() {
  for (;;) {
    std::function<void()> job;
    {
      std::unique_lock<std::mutex> lock(mutex_);
      ready_.wait(lock, [this] { return stopping_ || !jobs_.empty(); });
      if (stopping_ && jobs_.empty()) return;
      job = std::move(jobs_.front());
      jobs_.pop_front();
    }
    job();
  }
}

}  // namespace booth
