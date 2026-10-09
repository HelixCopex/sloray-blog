#!/usr/bin/env python3
"""用独立契约样例守护 CI 的兼容性检查边界。"""
import copy
import importlib.util
import pathlib
import unittest

spec=importlib.util.spec_from_file_location('check_api',pathlib.Path(__file__).with_name('check-api.py'))
api=importlib.util.module_from_spec(spec); spec.loader.exec_module(api)
def document():
    return {'openapi':'3.1.0','paths':{'/items/{id}':{
        'parameters':[{'$ref':'#/components/parameters/id'}],
        'get':{'responses':{'200':{'$ref':'#/components/responses/item'}}}}},
        'components':{'parameters':{'id':{'in':'path','name':'id','required':True,'schema':{'type':'string'}}},
        'responses':{'item':{'content':{'application/json':{'schema':{'type':'object','required':['name'],'properties':{'name':{'type':'string'}}}}}}}}}
class Compatibility(unittest.TestCase):
    def test_identical_and_new_endpoint_are_compatible(self):
        old=document(); new=copy.deepcopy(old)
        new['paths']['/new']={'get':{'responses':{'200':{}}}}
        self.assertEqual(api.compatible(old,new),[])
    def test_path_parameter_refs_cannot_change_type(self):
        old=document(); new=copy.deepcopy(old)
        new['components']['parameters']['id']['schema']['type']='integer'
        self.assertTrue(api.compatible(old,new))
    def test_response_ref_cannot_lose_required_field(self):
        old=document(); new=copy.deepcopy(old)
        new['components']['responses']['item']['content']['application/json']['schema']['required']=[]
        self.assertTrue(api.compatible(old,new))
    def test_new_required_query_parameter_is_breaking(self):
        old=document(); new=copy.deepcopy(old)
        new['paths']['/items/{id}']['get']['parameters']=[{'in':'query','name':'q','required':True,'schema':{'type':'string'}}]
        self.assertTrue(api.compatible(old,new))
    def test_new_authentication_requirement_is_breaking(self):
        old=document(); new=copy.deepcopy(old); new['security']=[{'session':[]}]
        self.assertTrue(api.compatible(old,new))
    def test_request_body_refs_are_compared(self):
        old=document()
        old['components']['requestBodies']={'write':{'content':{'application/json':{'schema':{'type':'object','properties':{'name':{'type':'string'}}}}}}}
        old['paths']['/items/{id}']['put']={'requestBody':{'$ref':'#/components/requestBodies/write'},'responses':{'204':{}}}
        new=copy.deepcopy(old); new['components']['requestBodies']['write']['required']=True
        self.assertTrue(api.compatible(old,new))
if __name__=='__main__': unittest.main()
