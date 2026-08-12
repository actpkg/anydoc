async def test_lists_the_three_tools(client):
    tools = await client.list_tools()
    names = [t.name for t in tools]
    assert len(tools) == 3
    assert "convert" in names
    assert "detect" in names
    assert "extract_assets" in names
