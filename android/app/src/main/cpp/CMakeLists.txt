cmake_minimum_required(VERSION 3.22.1)
project(decopad_uhid C)

# JNI library for the Shizuku user service path.
add_library(decopad_uhid SHARED uhid_jni.c)
target_link_libraries(decopad_uhid log)

# Standalone helper for ROMs whose Shizuku user service cannot start. It is
# packaged as a .so only so the Android Gradle plugin copies it into the APK's
# lib/<abi>/ directory, which shell is allowed to execute.
add_executable(decopad_uhid_helper uhid_helper.c)
set_target_properties(decopad_uhid_helper PROPERTIES
        OUTPUT_NAME decopad_uhid_helper
        PREFIX "lib"
        SUFFIX ".so")
