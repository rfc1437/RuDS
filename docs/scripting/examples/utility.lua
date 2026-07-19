function main()
    local project = bds.projects.get_active()
    if project == nil then return { processed = 0 } end
    local posts = bds.posts.get_all()
    bds.report_progress({ current = #posts, total = #posts, message = "Complete" })
    return { processed = #posts }
end
