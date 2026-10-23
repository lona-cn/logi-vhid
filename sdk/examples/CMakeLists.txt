cmake_minimum_required(VERSION 3.20)
project(logi_vhid_sdk_smoke LANGUAGES C CXX)

if(NOT WIN32 OR NOT MSVC OR NOT CMAKE_SIZEOF_VOID_P EQUAL 8)
    message(FATAL_ERROR "This SDK requires Windows x64 and MSVC.")
endif()

get_filename_component(LOGI_VHID_SDK_ROOT "${CMAKE_CURRENT_LIST_DIR}/.." ABSOLUTE)
add_library(logi_vhid SHARED IMPORTED)
set_target_properties(logi_vhid PROPERTIES
    IMPORTED_LOCATION "${LOGI_VHID_SDK_ROOT}/bin/logi_vhid.dll"
    IMPORTED_IMPLIB "${LOGI_VHID_SDK_ROOT}/lib/logi_vhid.dll.lib"
    INTERFACE_INCLUDE_DIRECTORIES "${LOGI_VHID_SDK_ROOT}/include"
)

enable_testing()
foreach(language IN ITEMS c cpp)
    add_executable(logi_vhid_smoke_${language} smoke.${language})
    target_link_libraries(logi_vhid_smoke_${language} PRIVATE logi_vhid)
    add_custom_command(TARGET logi_vhid_smoke_${language} POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            "$<TARGET_FILE:logi_vhid>"
            "$<TARGET_FILE_DIR:logi_vhid_smoke_${language}>"
        VERBATIM
    )
    add_test(NAME sdk_${language}_safe_smoke COMMAND logi_vhid_smoke_${language})
endforeach()
set_target_properties(logi_vhid_smoke_c PROPERTIES C_STANDARD 11 C_STANDARD_REQUIRED YES)
set_target_properties(logi_vhid_smoke_cpp PROPERTIES CXX_STANDARD 17 CXX_STANDARD_REQUIRED YES)
