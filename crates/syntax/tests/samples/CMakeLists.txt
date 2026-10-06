cmake_minimum_required(VERSION 3.20)
project(Greeting LANGUAGES CXX)

# The demo program.
set(CMAKE_CXX_STANDARD 20)
add_executable(greeting main.cpp)
if(WIN32)
  target_compile_definitions(greeting PRIVATE UNICODE)
endif()
message(STATUS "Building ${PROJECT_NAME}")

# Settings from the environment and the cache.
set(ENV{GREETING} "Hello\tworld\n")
set(DEMO_LEVEL 2 CACHE STRING "Demo level")

macro(add_demo target)
  add_executable(${target} ${ARGN})
endmacro()

function(show value)
  message(STATUS "value: ${value}")
endfunction()

add_demo(second second.cpp)
show(${DEMO_LEVEL})
message(STATUS "home: $ENV{HOME}, level: $CACHE{DEMO_LEVEL}")
